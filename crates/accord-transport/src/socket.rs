//! Abstraction de socket datagramme : implémentation UDP réelle et mesh
//! simulé en mémoire (perte, latence, churn) pour les tests d'intégration.

use async_trait::async_trait;
use std::io;
use std::net::SocketAddr;

/// Socket datagramme minimal utilisé par l'endpoint transport.
///
/// L'abstraction permet de faire tourner le protocole complet soit sur UDP,
/// soit sur un réseau simulé déterministe (SPEC §13 tests d'intégration).
#[async_trait]
pub trait DatagramSocket: Send + Sync + 'static {
    /// Envoie un datagramme à `dst`. Peut être silencieusement perdu (UDP).
    async fn send_to(&self, buf: &[u8], dst: SocketAddr) -> io::Result<usize>;

    /// Reçoit le prochain datagramme, rendant la source.
    async fn recv_from(&self) -> io::Result<(Vec<u8>, SocketAddr)>;

    /// Adresse locale liée.
    fn local_addr(&self) -> SocketAddr;
}

/// Implémentation UDP réelle sur `tokio::net::UdpSocket`.
pub struct UdpDatagram {
    inner: tokio::net::UdpSocket,
    local: SocketAddr,
}

impl UdpDatagram {
    /// Lie un socket UDP sur `bind` (ex. `0.0.0.0:0` pour un port éphémère).
    pub async fn bind(bind: SocketAddr) -> io::Result<Self> {
        let inner = tokio::net::UdpSocket::bind(bind).await?;
        let local = inner.local_addr()?;
        Ok(Self { inner, local })
    }

    /// Lie un socket UDP avec réutilisation d'adresse et de port
    /// (`SO_REUSEADDR`, plus `SO_REUSEPORT` sur Unix), prérequis du hole
    /// punching UDP (SPEC §11) : plusieurs tentatives de session et l'écoute
    /// principale peuvent alors partager le **même** port local éphémère,
    /// indispensable au simultaneous-open à travers un NAT.
    ///
    /// `SO_REUSEPORT` n'existe pas sur Windows : on se rabat sur le seul
    /// `SO_REUSEADDR`, suffisant pour relier plusieurs sockets au même port.
    pub fn bind_reuse(bind: SocketAddr) -> io::Result<Self> {
        use socket2::{Domain, Protocol, Socket, Type};

        let domain = if bind.is_ipv4() {
            Domain::IPV4
        } else {
            Domain::IPV6
        };
        let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        #[cfg(unix)]
        socket.set_reuse_port(true)?;
        socket.set_nonblocking(true)?;
        socket.bind(&bind.into())?;
        // `from_std` exige un socket non bloquant déjà configuré (ci-dessus).
        let std_sock: std::net::UdpSocket = socket.into();
        let inner = tokio::net::UdpSocket::from_std(std_sock)?;
        let local = inner.local_addr()?;
        Ok(Self { inner, local })
    }
}

#[async_trait]
impl DatagramSocket for UdpDatagram {
    async fn send_to(&self, buf: &[u8], dst: SocketAddr) -> io::Result<usize> {
        self.inner.send_to(buf, dst).await
    }

    async fn recv_from(&self) -> io::Result<(Vec<u8>, SocketAddr)> {
        // MTU applicative + marge (SPEC §13 : 1200 o UDP).
        let mut buf = vec![0u8; 2048];
        let (n, from) = self.inner.recv_from(&mut buf).await?;
        buf.truncate(n);
        Ok((buf, from))
    }

    fn local_addr(&self) -> SocketAddr {
        self.local
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bind_reuse_partage_le_meme_port() {
        // Premier socket sur un port éphémère, avec réutilisation.
        let a = UdpDatagram::bind_reuse("127.0.0.1:0".parse().unwrap()).unwrap();
        let port = a.local_addr().port();

        // Un second socket peut se lier au MÊME port (prérequis du hole
        // punching : écoute et tentatives partagent le port éphémère).
        // SO_REUSEPORT est requis côté Unix ; sur Windows SO_REUSEADDR suffit.
        #[cfg(unix)]
        {
            let bind: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
            let b = UdpDatagram::bind_reuse(bind).unwrap();
            assert_eq!(b.local_addr().port(), port);
        }
        #[cfg(not(unix))]
        {
            let _ = port;
        }
    }

    #[tokio::test]
    async fn bind_reuse_transmet_un_datagramme() {
        // Deux sockets réutilisables distincts s'échangent un datagramme :
        // vérifie que le socket socket2 → tokio reste pleinement fonctionnel.
        let a = UdpDatagram::bind_reuse("127.0.0.1:0".parse().unwrap()).unwrap();
        let b = UdpDatagram::bind_reuse("127.0.0.1:0".parse().unwrap()).unwrap();
        a.send_to(b"ping", b.local_addr()).await.unwrap();
        let (buf, from) = b.recv_from().await.unwrap();
        assert_eq!(&buf, b"ping");
        assert_eq!(from, a.local_addr());
    }
}

pub mod sim {
    //! Mesh UDP simulé, déterministe et paramétrable.

    use super::*;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;
    use tokio::sync::Mutex as AsyncMutex;

    /// Paramètres réseau injectés (perte, latence).
    #[derive(Debug, Clone, Copy)]
    pub struct NetConditions {
        /// Probabilité de perte d'un datagramme [0.0, 1.0].
        pub loss: f64,
        /// Latence minimale ajoutée (ms).
        pub latency_min_ms: u64,
        /// Latence maximale ajoutée (ms).
        pub latency_max_ms: u64,
    }

    impl Default for NetConditions {
        fn default() -> Self {
            Self {
                loss: 0.0,
                latency_min_ms: 0,
                latency_max_ms: 0,
            }
        }
    }

    /// Datagramme livré à un socket simulé : `(charge, source)`.
    type Datagram = (Vec<u8>, SocketAddr);
    type Inbox = mpsc::UnboundedSender<Datagram>;

    /// Premier port externe attribué par un NAT simulé (croissant ensuite :
    /// mapping distinct par destination, sémantique symétrique).
    const NAT_FIRST_PORT: u16 = 50_000;

    /// État d'un NAT symétrique simulé devant un nœud : chaque destination
    /// contactée obtient son propre mapping externe `(ip_externe, port frais)`.
    /// Un mapping n'accepte en entrant QUE des datagrammes émanant exactement
    /// de sa destination d'origine (filtrage adresse+port, le plus strict).
    /// L'adresse interne du nœud n'est jamais joignable de l'extérieur.
    struct NatState {
        external_ip: std::net::IpAddr,
        /// destination → adresse externe du mapping.
        mappings: HashMap<SocketAddr, SocketAddr>,
        next_port: u16,
    }

    #[derive(Default)]
    struct Fabric {
        inboxes: HashMap<SocketAddr, Inbox>,
        conditions: HashMap<SocketAddr, NetConditions>,
        down: HashMap<SocketAddr, bool>,
        /// NAT symétrique par nœud (clé : adresse interne).
        nat: HashMap<SocketAddr, NatState>,
        /// Adresse externe d'un mapping → `(interne, destination d'origine)`.
        nat_reverse: HashMap<SocketAddr, (SocketAddr, SocketAddr)>,
    }

    /// Réseau simulé partagé entre plusieurs [`SimSocket`].
    #[derive(Clone)]
    pub struct SimNet {
        fabric: Arc<Mutex<Fabric>>,
        rng: Arc<Mutex<StdRng>>,
        default_conditions: NetConditions,
    }

    impl SimNet {
        /// Crée un réseau simulé déterministe à partir d'une graine.
        pub fn new(seed: u64, conditions: NetConditions) -> Self {
            Self {
                fabric: Arc::new(Mutex::new(Fabric::default())),
                rng: Arc::new(Mutex::new(StdRng::seed_from_u64(seed))),
                default_conditions: conditions,
            }
        }

        /// Enregistre un nœud à `addr` et rend son socket.
        pub fn bind(&self, addr: SocketAddr) -> SimSocket {
            let (tx, rx) = mpsc::unbounded_channel();
            let mut f = self.fabric.lock().expect("fabric mutex");
            f.inboxes.insert(addr, tx);
            f.conditions.insert(addr, self.default_conditions);
            f.down.insert(addr, false);
            SimSocket {
                net: self.clone(),
                local: addr,
                rx: Arc::new(AsyncMutex::new(rx)),
            }
        }

        /// Simule une coupure réseau d'un nœud (churn).
        pub fn set_down(&self, addr: SocketAddr, down: bool) {
            let mut f = self.fabric.lock().expect("fabric mutex");
            f.down.insert(addr, down);
        }

        /// Ajuste les conditions réseau d'un nœud spécifique.
        pub fn set_conditions(&self, addr: SocketAddr, c: NetConditions) {
            let mut f = self.fabric.lock().expect("fabric mutex");
            f.conditions.insert(addr, c);
        }

        /// Place un nœud (adresse interne `internal`) derrière un NAT
        /// SYMÉTRIQUE simulé d'IP externe `external_ip` : mapping par
        /// destination, filtrage entrant strict, aucun entrant non sollicité.
        pub fn set_symmetric_nat(&self, internal: SocketAddr, external_ip: std::net::IpAddr) {
            let mut f = self.fabric.lock().expect("fabric mutex");
            f.nat.insert(
                internal,
                NatState {
                    external_ip,
                    mappings: HashMap::new(),
                    next_port: NAT_FIRST_PORT,
                },
            );
        }

        fn deliver(&self, from: SocketAddr, to: SocketAddr, buf: Vec<u8>) {
            let (inbox, src, delay) = {
                let mut f = self.fabric.lock().expect("fabric mutex");
                if *f.down.get(&from).unwrap_or(&false) {
                    return;
                }
                // Source effective : l'adresse du mapping NAT (créé à la
                // demande, un par destination — sémantique symétrique) si
                // l'émetteur est NATé, son adresse interne sinon.
                let src = match f.nat.get_mut(&from) {
                    Some(nat) => match nat.mappings.get(&to) {
                        Some(mapped) => *mapped,
                        None => {
                            let mapped = SocketAddr::new(nat.external_ip, nat.next_port);
                            nat.next_port = nat.next_port.wrapping_add(1);
                            nat.mappings.insert(to, mapped);
                            f.nat_reverse.insert(mapped, (from, to));
                            mapped
                        }
                    },
                    None => from,
                };
                // Destination effective : la traduction inverse si `to` est un
                // mapping NAT — livré à l'adresse interne SEULEMENT si la
                // source est exactement la destination d'origine du mapping
                // (filtrage adresse+port du NAT symétrique). L'adresse interne
                // d'un nœud NATé n'est pas joignable de l'extérieur.
                let dest = match f.nat_reverse.get(&to) {
                    Some((internal, expected_remote)) => {
                        if src != *expected_remote {
                            return; // entrant non sollicité : jeté par le NAT
                        }
                        *internal
                    }
                    None => {
                        if f.nat.contains_key(&to) {
                            return; // adresse interne injoignable de l'extérieur
                        }
                        to
                    }
                };
                if *f.down.get(&dest).unwrap_or(&false) {
                    return;
                }
                let cond = f.conditions.get(&dest).copied().unwrap_or_default();
                let mut rng = self.rng.lock().expect("rng mutex");
                if rng.gen::<f64>() < cond.loss {
                    return; // datagramme perdu
                }
                let delay = if cond.latency_max_ms > cond.latency_min_ms {
                    rng.gen_range(cond.latency_min_ms..=cond.latency_max_ms)
                } else {
                    cond.latency_min_ms
                };
                match f.inboxes.get(&dest) {
                    Some(tx) => (tx.clone(), src, delay),
                    None => return,
                }
            };
            if delay == 0 {
                let _ = inbox.send((buf, src));
            } else {
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    let _ = inbox.send((buf, src));
                });
            }
        }
    }

    /// Socket rattaché à un [`SimNet`].
    pub struct SimSocket {
        net: SimNet,
        local: SocketAddr,
        rx: Arc<AsyncMutex<mpsc::UnboundedReceiver<Datagram>>>,
    }

    #[async_trait]
    impl DatagramSocket for SimSocket {
        async fn send_to(&self, buf: &[u8], dst: SocketAddr) -> io::Result<usize> {
            let n = buf.len();
            self.net.deliver(self.local, dst, buf.to_vec());
            Ok(n)
        }

        async fn recv_from(&self) -> io::Result<(Vec<u8>, SocketAddr)> {
            let mut rx = self.rx.lock().await;
            rx.recv()
                .await
                .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "sim closed"))
        }

        fn local_addr(&self) -> SocketAddr {
            self.local
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::time::Duration;

        fn addr(s: &str) -> SocketAddr {
            s.parse().unwrap()
        }

        async fn recv_or_timeout(sock: &SimSocket) -> Option<Datagram> {
            tokio::time::timeout(Duration::from_millis(100), sock.recv_from())
                .await
                .ok()
                .and_then(Result::ok)
        }

        /// Sémantique du NAT symétrique simulé : mapping par destination,
        /// filtrage entrant strict, adresse interne injoignable.
        #[tokio::test]
        async fn nat_symetrique_mapping_par_destination_et_filtrage() {
            let net = SimNet::new(42, NetConditions::default());
            let nated = net.bind(addr("10.0.0.2:4001"));
            let pub1 = net.bind(addr("127.1.0.1:4000"));
            let pub2 = net.bind(addr("127.2.0.1:4000"));
            net.set_symmetric_nat(nated.local_addr(), "127.9.0.1".parse().unwrap());

            // Sortant vers deux destinations : deux mappings DISTINCTS, jamais
            // l'adresse interne.
            nated.send_to(b"a", pub1.local_addr()).await.unwrap();
            nated.send_to(b"b", pub2.local_addr()).await.unwrap();
            let (_, m1) = recv_or_timeout(&pub1).await.expect("reçu par pub1");
            let (_, m2) = recv_or_timeout(&pub2).await.expect("reçu par pub2");
            assert_eq!(m1.ip(), "127.9.0.1".parse::<std::net::IpAddr>().unwrap());
            assert_ne!(m1, m2, "mapping par destination (symétrique)");
            assert_ne!(m1, nated.local_addr());

            // Retour depuis la destination d'origine : traverse le mapping.
            pub1.send_to(b"pong", m1).await.unwrap();
            let (buf, from) = recv_or_timeout(&nated).await.expect("réponse reçue");
            assert_eq!(&buf, b"pong");
            assert_eq!(from, pub1.local_addr());

            // Un TIERS qui vise le mapping d'un autre pair : jeté.
            pub2.send_to(b"intrus", m1).await.unwrap();
            assert!(recv_or_timeout(&nated).await.is_none(), "filtrage strict");

            // L'adresse interne n'est jamais joignable de l'extérieur.
            pub1.send_to(b"direct", nated.local_addr()).await.unwrap();
            assert!(
                recv_or_timeout(&nated).await.is_none(),
                "interne inaccessible"
            );

            // Le mapping est STABLE pour une même destination.
            nated.send_to(b"c", pub1.local_addr()).await.unwrap();
            let (_, m1bis) = recv_or_timeout(&pub1).await.expect("reçu");
            assert_eq!(m1, m1bis);
        }

        /// Deux nœuds derrière deux NAT symétriques distincts ne peuvent PAS
        /// se poinçonner : les salves croisées vers les mappings observés par
        /// un tiers sont toutes jetées (mapping lié à sa destination d'origine).
        #[tokio::test]
        async fn nat_symetrique_croise_bloque_le_poinconnage() {
            let net = SimNet::new(7, NetConditions::default());
            let a = net.bind(addr("10.0.0.2:4001"));
            let b = net.bind(addr("10.0.1.2:4002"));
            let rdv = net.bind(addr("127.1.0.1:4000"));
            net.set_symmetric_nat(a.local_addr(), "127.8.0.1".parse().unwrap());
            net.set_symmetric_nat(b.local_addr(), "127.9.0.1".parse().unwrap());

            // Chacun contacte le point de rendez-vous, qui observe les mappings.
            a.send_to(b"hi", rdv.local_addr()).await.unwrap();
            b.send_to(b"hi", rdv.local_addr()).await.unwrap();
            let (_, map_a) = recv_or_timeout(&rdv).await.unwrap();
            let (_, map_b) = recv_or_timeout(&rdv).await.unwrap();

            // Salves croisées vers les mappings observés : aucune ne passe
            // (la source de A vers map_b est un NOUVEAU mapping ≠ rdv).
            a.send_to(b"punch", map_b).await.unwrap();
            b.send_to(b"punch", map_a).await.unwrap();
            assert!(recv_or_timeout(&a).await.is_none());
            assert!(recv_or_timeout(&b).await.is_none());

            // Le chemin par le rendez-vous, lui, reste ouvert dans les deux sens.
            rdv.send_to(b"ok-a", map_a).await.unwrap();
            rdv.send_to(b"ok-b", map_b).await.unwrap();
            assert_eq!(recv_or_timeout(&a).await.unwrap().0, b"ok-a");
            assert_eq!(recv_or_timeout(&b).await.unwrap().0, b"ok-b");
        }
    }
}
