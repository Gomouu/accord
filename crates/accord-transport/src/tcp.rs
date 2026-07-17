//! Repli TCP du transport (SPEC §11.3) : datagrammes encadrés sur flux TCP.
//!
//! Quand le poinçonnage UDP échoue (UDP filtré, NAT récalcitrant), le même
//! protocole paquet (HELLO/WELCOME/DATA, ≤ 1200 o) peut transiter sur une
//! connexion TCP « poinçonnée » par ouverture simultanée : les deux pairs
//! tentent `connect()` l'un vers l'autre depuis leur port P2P local (avec
//! `SO_REUSEADDR`/`SO_REUSEPORT`), pendant qu'un accepteur écoute sur ce même
//! port. Les SYN qui se croisent ouvrent les mappings NAT ; la première
//! connexion établie gagne.
//!
//! L'intégration est transparente pour l'[`crate::endpoint::Endpoint`] : le
//! [`MuxSocket`] implémente [`DatagramSocket`] au-dessus du socket UDP réel et
//! d'un registre de liens TCP ([`TcpLinks`]). Un datagramme vers une adresse
//! couverte par un lien TCP part encadré sur le flux ; tout le reste part en
//! UDP. Les trames reçues d'un lien TCP sont remontées comme des datagrammes
//! ordinaires, avec l'adresse du pair TCP comme source. Le handshake, le
//! chiffrement, les keep-alive et l'anti-DoS de l'endpoint s'appliquent donc à
//! l'identique sur les deux chemins.
//!
//! **Sécurité (surface réseau publique)** :
//! - chaque trame est bornée (`1..=`[`MAX_FRAME`] octets) : une longueur hors
//!   borne ferme le lien, aucune allocation pilotée par l'attaquant ;
//! - pas d'amplification : TCP prouve l'adresse source (poignée de main en
//!   trois temps), et rien n'est émis vers un pair qui n'a rien demandé ;
//! - le nombre de liens simultanés est plafonné ([`MAX_LINKS`]) et chaque lien
//!   inactif est fermé après [`LINK_IDLE_TIMEOUT`] (anti « slow-loris ») ;
//! - un lien ne contourne aucune validation : les octets remontés passent par
//!   le même décodage strict et le même PoW/cookie que les datagrammes UDP.
//!
//! **Limite documentée** : à travers deux NAT, le port TCP public de chaque
//! pair est en général différent de son port UDP observé et n'est pas
//! découvrable sans observation TCP dédiée. Le poinçonnage TCP réussit surtout
//! avec des NAT préservant le port, une redirection manuelle, ou sur un chemin
//! où seul UDP est filtré. C'est un repli best-effort (SPEC §11.3) : le relais
//! reste le dernier recours.

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::{mpsc, watch, Mutex as AsyncMutex};

use crate::socket::DatagramSocket;

/// Taille maximale d'une trame (charge utile d'un datagramme encapsulé).
/// Miroir du tampon de réception UDP : le protocole n'émet jamais plus.
pub const MAX_FRAME: usize = 2048;

/// Nombre maximal de liens TCP simultanés (entrants + sortants). Au-delà, les
/// nouvelles connexions sont refusées : borne mémoire stricte.
pub const MAX_LINKS: usize = 64;

/// Nombre maximal de liens simultanés PAR ADRESSE IP distante : freine
/// l'épuisement du registre par un hôte unique (un botnet distribué reste
/// capable de saturer le REPLI TCP — les chemins UDP et relais ne passent pas
/// par ce registre et restent intacts ; risque résiduel documenté).
pub const MAX_LINKS_PER_IP: usize = 4;

/// Trames en attente d'écriture par lien. File pleine ⇒ trame perdue,
/// sémantique datagramme (les retransmissions du protocole compensent).
const LINK_SEND_QUEUE: usize = 256;

/// Trames entrantes en attente de lecture par l'endpoint (tous liens
/// confondus). File pleine ⇒ trame perdue (sémantique datagramme).
const INBOUND_QUEUE: usize = 1024;

/// Inactivité maximale d'un lien (aucune trame lue) avant fermeture. Les
/// keep-alive de session (25 s) maintiennent les liens utiles bien en deçà.
const LINK_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

/// Identifiant interne d'un lien (dissocie un lien mort de son remplaçant).
type LinkId = u64;

/// Extrémité d'écriture d'un lien enregistré.
struct LinkHandle {
    id: LinkId,
    tx: mpsc::Sender<Vec<u8>>,
    close: watch::Sender<bool>,
}

/// Registre des liens TCP actifs, indexés par l'adresse du pair distant.
pub struct TcpLinks {
    links: Mutex<HashMap<SocketAddr, LinkHandle>>,
    inbound: mpsc::Sender<(Vec<u8>, SocketAddr)>,
    next_id: AtomicU64,
}

impl TcpLinks {
    /// Vrai si un lien TCP actif couvre `addr`.
    pub fn contains(&self, addr: &SocketAddr) -> bool {
        self.links
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(addr)
    }

    /// Nombre de liens actifs.
    pub fn len(&self) -> usize {
        self.links.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Vrai si aucun lien n'est actif.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Adopte une connexion TCP établie (entrante ou poinçonnée) : enregistre
    /// le lien sous l'adresse du pair et lance ses tâches de lecture/écriture.
    /// Un lien existant vers la même adresse est remplacé (fermé proprement) :
    /// la connexion la plus récente reflète l'état NAT le plus frais.
    ///
    /// Refuse (en fermant la connexion) si le plafond [`MAX_LINKS`] est
    /// atteint — borne stricte de la surface exposée publiquement.
    pub fn adopt(self: &Arc<Self>, stream: TcpStream) -> io::Result<SocketAddr> {
        let peer = stream.peer_addr()?;
        // Pas de Nagle : les trames sont petites et sensibles à la latence.
        let _ = stream.set_nodelay(true);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel(LINK_SEND_QUEUE);
        let (close_tx, close_rx) = watch::channel(false);
        {
            let mut links = self.links.lock().unwrap_or_else(|e| e.into_inner());
            let existing: Vec<SocketAddr> = links.keys().copied().collect();
            if let Err(reason) = admit(&existing, &peer) {
                return Err(io::Error::new(io::ErrorKind::OutOfMemory, reason));
            }
            if let Some(old) = links.insert(
                peer,
                LinkHandle {
                    id,
                    tx,
                    close: close_tx,
                },
            ) {
                // Remplacement : signale la fermeture de l'ancien lien (ses
                // tâches s'arrêtent, le flux est lâché).
                let _ = old.close.send(true);
            }
        }
        let (read_half, write_half) = stream.into_split();
        tokio::spawn(Arc::clone(self).read_loop(read_half, peer, id, close_rx));
        tokio::spawn(write_loop(write_half, rx));
        Ok(peer)
    }

    /// Retire `peer` du registre si l'entrée correspond toujours au lien `id`
    /// (un remplaçant plus récent n'est jamais délogé par la mort de l'ancien).
    fn unregister(&self, peer: SocketAddr, id: LinkId) {
        let mut links = self.links.lock().unwrap_or_else(|e| e.into_inner());
        if links.get(&peer).is_some_and(|h| h.id == id) {
            links.remove(&peer);
        }
    }

    /// Émet `buf` sur le lien couvrant `dst`, s'il existe. Rend :
    /// - `Some(Ok(n))` : trame remise à la file d'écriture (ou perdue si la
    ///   file est pleine — sémantique datagramme) ;
    /// - `Some(Err(_))` jamais : un lien mort est désenregistré et l'appelant
    ///   reçoit `None` pour retenter en UDP ;
    /// - `None` : aucun lien pour `dst`.
    fn try_send(&self, buf: &[u8], dst: SocketAddr) -> Option<usize> {
        let mut links = self.links.lock().unwrap_or_else(|e| e.into_inner());
        let handle = links.get(&dst)?;
        match handle.tx.try_send(buf.to_vec()) {
            Ok(()) => Some(buf.len()),
            // File pleine : trame perdue, comme un datagramme UDP sous
            // congestion. Le lien reste en place.
            Err(mpsc::error::TrySendError::Full(_)) => Some(buf.len()),
            // Lien mort : on le retire et l'appelant bascule sur UDP.
            Err(mpsc::error::TrySendError::Closed(_)) => {
                links.remove(&dst);
                None
            }
        }
    }

    /// Boucle de lecture d'un lien : trames `[len: u16 BE][charge]` strictement
    /// bornées, remontées dans la file entrante commune. Toute violation
    /// (longueur nulle ou > [`MAX_FRAME`]), erreur d'E/S, inactivité prolongée
    /// ou signal de fermeture met fin au lien.
    async fn read_loop(
        self: Arc<Self>,
        mut read: OwnedReadHalf,
        peer: SocketAddr,
        id: LinkId,
        mut close: watch::Receiver<bool>,
    ) {
        loop {
            let frame = tokio::select! {
                r = tokio::time::timeout(LINK_IDLE_TIMEOUT, read_frame(&mut read)) => match r {
                    Ok(Ok(frame)) => frame,
                    Ok(Err(e)) => {
                        tracing::debug!(?peer, erreur = %e, "tcp : lien fermé");
                        break;
                    }
                    Err(_) => {
                        tracing::debug!(?peer, "tcp : lien inactif, fermé");
                        break;
                    }
                },
                res = close.changed() => {
                    if res.is_err() || *close.borrow() {
                        break;
                    }
                    continue;
                }
            };
            // File entrante pleine : trame perdue (sémantique datagramme),
            // jamais d'attente bloquante pilotée par le réseau.
            let _ = self.inbound.try_send((frame, peer));
        }
        self.unregister(peer, id);
    }
}

/// Décision d'admission d'un nouveau lien `peer` face aux liens `existing`
/// (fonction pure, testable sans réseau) : le remplacement d'un lien vers la
/// même adresse passe toujours ; sinon borne globale ([`MAX_LINKS`]) puis
/// borne par IP distante ([`MAX_LINKS_PER_IP`]).
fn admit(existing: &[SocketAddr], peer: &SocketAddr) -> Result<(), &'static str> {
    if existing.contains(peer) {
        return Ok(()); // remplacement d'un lien existant : jamais compté double
    }
    if existing.len() >= MAX_LINKS {
        return Err("plafond de liens TCP atteint");
    }
    if existing.iter().filter(|a| a.ip() == peer.ip()).count() >= MAX_LINKS_PER_IP {
        return Err("plafond de liens TCP par IP atteint");
    }
    Ok(())
}

/// Lit une trame complète : longueur `u16` big-endian puis charge. Longueur
/// hors borne ⇒ erreur (le lien sera fermé, aucune allocation démesurée).
async fn read_frame(read: &mut OwnedReadHalf) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 2];
    read.read_exact(&mut len_buf).await?;
    let len = u16::from_be_bytes(len_buf) as usize;
    if len == 0 || len > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "longueur de trame hors borne",
        ));
    }
    let mut frame = vec![0u8; len];
    read.read_exact(&mut frame).await?;
    Ok(frame)
}

/// Boucle d'écriture d'un lien : encadre et émet chaque trame de la file.
/// S'arrête quand la file est fermée (lien désenregistré ou remplacé) ou sur
/// erreur d'écriture.
async fn write_loop(mut write: OwnedWriteHalf, mut rx: mpsc::Receiver<Vec<u8>>) {
    while let Some(frame) = rx.recv().await {
        debug_assert!(!frame.is_empty() && frame.len() <= MAX_FRAME);
        let len = (frame.len() as u16).to_be_bytes();
        if write.write_all(&len).await.is_err() || write.write_all(&frame).await.is_err() {
            return;
        }
    }
    let _ = write.shutdown().await;
}

/// Socket multiplexé UDP + liens TCP, présenté à l'endpoint comme un unique
/// [`DatagramSocket`]. Voir la documentation du module.
pub struct MuxSocket {
    udp: Arc<dyn DatagramSocket>,
    links: Arc<TcpLinks>,
    inbound: AsyncMutex<mpsc::Receiver<(Vec<u8>, SocketAddr)>>,
}

impl MuxSocket {
    /// Construit le multiplexeur au-dessus du socket UDP réel. Rend le socket
    /// (à passer à l'endpoint) et le registre de liens (à alimenter par
    /// l'accepteur TCP et le poinçonnage).
    pub fn new(udp: Arc<dyn DatagramSocket>) -> (Arc<Self>, Arc<TcpLinks>) {
        let (inbound_tx, inbound_rx) = mpsc::channel(INBOUND_QUEUE);
        let links = Arc::new(TcpLinks {
            links: Mutex::new(HashMap::new()),
            inbound: inbound_tx,
            next_id: AtomicU64::new(1),
        });
        let mux = Arc::new(Self {
            udp,
            links: Arc::clone(&links),
            inbound: AsyncMutex::new(inbound_rx),
        });
        (mux, links)
    }
}

#[async_trait]
impl DatagramSocket for MuxSocket {
    async fn send_to(&self, buf: &[u8], dst: SocketAddr) -> io::Result<usize> {
        // Trame trop grande pour l'encadrement TCP : UDP direct (n'arrive pas
        // en pratique, le protocole borne ses datagrammes bien en deçà).
        if buf.len() <= MAX_FRAME {
            if let Some(n) = self.links.try_send(buf, dst) {
                return Ok(n);
            }
        }
        self.udp.send_to(buf, dst).await
    }

    async fn recv_from(&self) -> io::Result<(Vec<u8>, SocketAddr)> {
        let mut inbound = self.inbound.lock().await;
        // Les deux branches sont annulables sans perte (recv_from UDP tokio et
        // recv mpsc sont « cancel-safe »).
        tokio::select! {
            r = self.udp.recv_from() => r,
            f = inbound.recv() => f.ok_or_else(|| {
                io::Error::new(io::ErrorKind::BrokenPipe, "mux tcp fermé")
            }),
        }
    }

    fn local_addr(&self) -> SocketAddr {
        self.udp.local_addr()
    }
}

/// Tente une connexion TCP « poinçonnée » vers `remote` DEPUIS le port local
/// `local_port` (ouverture simultanée, SPEC §11.3) : le socket est lié avec
/// `SO_REUSEADDR` (+ `SO_REUSEPORT` sur Unix) pour partager le port avec
/// l'accepteur et les autres tentatives. Une seule tentative, bornée par
/// `timeout` ; l'appelant orchestre les répétitions et l'arrêt à la première
/// connexion établie (la sienne ou celle acceptée en face).
pub async fn punch_connect(
    local_port: u16,
    remote: SocketAddr,
    timeout: Duration,
) -> io::Result<TcpStream> {
    let (socket, bind_addr): (TcpSocket, SocketAddr) = if remote.is_ipv4() {
        (
            TcpSocket::new_v4()?,
            format!("0.0.0.0:{local_port}").parse().expect("addr v4"),
        )
    } else {
        (
            TcpSocket::new_v6()?,
            format!("[::]:{local_port}").parse().expect("addr v6"),
        )
    };
    socket.set_reuseaddr(true)?;
    #[cfg(unix)]
    socket.set_reuseport(true)?;
    socket.bind(bind_addr)?;
    match tokio::time::timeout(timeout, socket.connect(remote)).await {
        Ok(res) => res,
        Err(_) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "connexion poinçonnée hors délai",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::socket::UdpDatagram;

    async fn mux_pair() -> (Arc<MuxSocket>, Arc<TcpLinks>, SocketAddr) {
        let udp = Arc::new(
            UdpDatagram::bind("127.0.0.1:0".parse().unwrap())
                .await
                .unwrap(),
        );
        let addr = udp.local_addr();
        let (mux, links) = MuxSocket::new(udp);
        (mux, links, addr)
    }

    #[tokio::test]
    async fn mux_transmet_en_udp_sans_lien() {
        let (a, _la, addr_a) = mux_pair().await;
        let (b, _lb, addr_b) = mux_pair().await;
        a.send_to(b"ping", addr_b).await.unwrap();
        let (buf, from) = b.recv_from().await.unwrap();
        assert_eq!(&buf, b"ping");
        assert_eq!(from, addr_a);
    }

    #[tokio::test]
    async fn mux_route_sur_le_lien_tcp_et_remonte_les_trames() {
        let (a, links_a, _) = mux_pair().await;
        let (b, links_b, _) = mux_pair().await;

        // Connexion TCP réelle entre les deux registres (loopback).
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = listener.local_addr().unwrap();
        let (client, server) = tokio::join!(TcpStream::connect(target), listener.accept());
        let (server_stream, _) = server.unwrap();
        let peer_b = links_a.adopt(client.unwrap()).unwrap();
        let peer_a = links_b.adopt(server_stream).unwrap();

        // A → B sur le lien : la trame ressort chez B comme un datagramme
        // portant l'adresse TCP de A.
        a.send_to(b"par tcp", peer_b).await.unwrap();
        let (buf, from) = b.recv_from().await.unwrap();
        assert_eq!(&buf, b"par tcp");
        assert_eq!(from, peer_a);

        // Retour B → A.
        b.send_to(b"retour", peer_a).await.unwrap();
        let (buf, from) = a.recv_from().await.unwrap();
        assert_eq!(&buf, b"retour");
        assert_eq!(from, peer_b);
    }

    #[tokio::test]
    async fn trame_hors_borne_ferme_le_lien_sans_panique() {
        let (_a, links, _) = mux_pair().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = listener.local_addr().unwrap();
        let (client, server) = tokio::join!(TcpStream::connect(target), listener.accept());
        let (server_stream, _) = server.unwrap();
        let peer = links.adopt(server_stream).unwrap();
        assert!(links.contains(&peer));

        // L'attaquant annonce une trame de 65535 octets (> MAX_FRAME) : le
        // lien doit se fermer sans allocation démesurée ni panique.
        let mut attacker = client.unwrap();
        attacker.write_all(&u16::MAX.to_be_bytes()).await.unwrap();
        attacker.write_all(&[0u8; 64]).await.unwrap();

        // Le lien finit désenregistré.
        for _ in 0..50 {
            if !links.contains(&peer) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("lien non fermé après trame hors borne");
    }

    #[tokio::test]
    async fn longueur_nulle_ferme_le_lien() {
        let (_a, links, _) = mux_pair().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = listener.local_addr().unwrap();
        let (client, server) = tokio::join!(TcpStream::connect(target), listener.accept());
        let peer = links.adopt(server.unwrap().0).unwrap();
        let mut attacker = client.unwrap();
        attacker.write_all(&0u16.to_be_bytes()).await.unwrap();
        for _ in 0..50 {
            if !links.contains(&peer) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("lien non fermé après longueur nulle");
    }

    #[test]
    fn admission_borne_globale_et_par_ip() {
        // Borne globale : registre plein, tout nouveau pair est refusé.
        let full: Vec<SocketAddr> = (0..MAX_LINKS).map(|i| addr_n(i as u8 + 1, 1000)).collect();
        assert!(admit(&full, &addr_n(200, 9)).is_err());
        // ... sauf le remplacement d'un lien existant.
        assert!(admit(&full, &full[0]).is_ok());

        // Borne par IP : un hôte unique ne peut pas accaparer le registre.
        let one_host: Vec<SocketAddr> = (0..MAX_LINKS_PER_IP)
            .map(|i| addr_n(7, 1000 + i as u16))
            .collect();
        assert!(
            admit(&one_host, &addr_n(7, 2000)).is_err(),
            "5e lien même IP refusé"
        );
        assert!(
            admit(&one_host, &addr_n(8, 2000)).is_ok(),
            "autre IP acceptée"
        );
    }

    fn addr_n(last_octet: u8, port: u16) -> SocketAddr {
        SocketAddr::new(std::net::IpAddr::V4([203, 0, 113, last_octet].into()), port)
    }

    #[tokio::test]
    async fn plafond_par_ip_refuse_le_surplus_en_reel() {
        // Tous les liens émanent de 127.0.0.1 : la borne PAR IP s'applique.
        let (_a, links, _) = mux_pair().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = listener.local_addr().unwrap();
        let mut kept = Vec::new();
        for i in 0..=MAX_LINKS_PER_IP {
            let (client, server) = tokio::join!(TcpStream::connect(target), listener.accept());
            let res = links.adopt(server.unwrap().0);
            if i < MAX_LINKS_PER_IP {
                assert!(res.is_ok(), "lien {i} sous le plafond accepté");
                kept.push(client.unwrap()); // garde le client ouvert
            } else {
                assert!(res.is_err(), "lien au-delà du plafond par IP refusé");
            }
        }
        assert_eq!(links.len(), MAX_LINKS_PER_IP);
    }

    #[tokio::test]
    async fn punch_connect_ouverture_simultanee_loopback() {
        // Deux pairs se `connect()` mutuellement depuis leur port lié : sur
        // loopback, l'ouverture simultanée TCP aboutit (les SYN se croisent)
        // ou l'un des deux accepte via son écouteur — ici on vérifie la voie
        // accepteur.
        //
        // L'écouteur qui occupe le port P2P local DOIT poser les mêmes options
        // de partage (`SO_REUSEADDR` + `SO_REUSEPORT`) que `punch_connect`,
        // sinon Linux refuse le re-bind du port (`EADDRINUSE`) : le poinçonnage
        // échouait et l'`accept()` non borné attendait alors indéfiniment — le
        // test passait sur macOS (REUSEPORT permissif) mais figeait la CI Linux.
        let l_a = {
            let s = TcpSocket::new_v4().unwrap();
            s.set_reuseaddr(true).unwrap();
            #[cfg(unix)]
            s.set_reuseport(true).unwrap();
            s.bind("0.0.0.0:0".parse().unwrap()).unwrap();
            s.listen(16).unwrap()
        };
        let port_a = l_a.local_addr().unwrap().port();
        let l_b = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr_b = l_b.local_addr().unwrap();

        // `accept()` borné : la voie accepteur ne doit jamais bloquer le test à
        // l'infini si la pile loopback de la plateforme ne livre pas.
        let (out, inc) = tokio::join!(
            punch_connect(port_a, addr_b, Duration::from_secs(2)),
            tokio::time::timeout(Duration::from_secs(3), l_b.accept()),
        );
        // Garantie essentielle : la connexion sortante émane bien du port P2P
        // local (le port stable réutilisé), prérequis du poinçonnage.
        let out = out.expect("poinçonnage sortant établi depuis le port partagé");
        assert_eq!(out.local_addr().unwrap().port(), port_a);
        // Voie accepteur : même port source si la plateforme l'a livrée avant
        // le délai (tolérant pour rester robuste multi-plateforme).
        if let Ok(Ok((inc, from))) = inc {
            assert_eq!(from.port(), port_a);
            drop(inc);
        }
        drop(l_a);
    }
}
