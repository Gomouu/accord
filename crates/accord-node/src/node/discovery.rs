//! Découverte de pairs Accord sur le réseau local via mDNS ([`mdns_sd`]).
//!
//! Le nœud **annonce** un service `_accord._udp.local.` portant sa clé publique
//! (propriété TXT) et son port P2P, et **parcourt** le même service pour trouver
//! les autres nœuds Accord du LAN. Chaque pair découvert est ajouté au carnet
//! d'adresses joignables (comme un pair d'amorçage), de sorte que deux amis sur
//! le même Wi-Fi se connectent sans aucune configuration.
//!
//! La découverte est désactivable et ne bloque jamais le démarrage : le démon
//! mDNS tourne sur son propre fil, et une tâche tokio consomme ses événements
//! jusqu'au signal d'arrêt (annonce retirée proprement à l'arrêt).
//!
//! **Résilience.** Le démon de la lib peut paniquer sur son propre fil, hors de
//! notre contrôle (p. ex. une annonce LAN malformée qui déclenche une assertion
//! interne — voir <https://github.com/keepsimple1/mdns-sd/issues/483>). Quand ce
//! fil meurt, le canal d'événements se déconnecte : nous le détectons, journalons
//! l'incident et **recréons** le démon avec un backoff exponentiel plafonné, au
//! lieu de perdre la découverte en silence. La logique est isolée derrière un
//! [`MdnsBackend`]/[`MdnsSession`] pour tester la relance sans matériel réseau.
//!
//! La logique parsable (nom d'instance, propriétés TXT, extraction d'un pair
//! depuis une annonce résolue, extraction de clé depuis un nom complet) est
//! isolée en fonctions pures testables sans réseau ; la découverte réelle sur
//! le LAN n'est vérifiable qu'en intégration.

use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::watch;

use crate::hex;

/// Type de service mDNS annoncé et parcouru pour Accord.
pub const SERVICE_TYPE: &str = "_accord._udp.local.";
/// Clé de la propriété TXT portant la clé publique (hex) du nœud.
pub const TXT_PUBKEY: &str = "pk";

/// Délai initial avant de recréer le démon mDNS après une mort inattendue.
const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
/// Plafond du backoff exponentiel de relance : borne le rythme des tentatives
/// si une annonce LAN empoisonnée tue le démon à chaque redémarrage (au pire
/// une relance + un avertissement toutes les [`BACKOFF_MAX`], pas de boucle
/// serrée). La découverte LAN n'étant pas sensible à la latence, 30 s convient.
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Rappel (synchrone, bref) déclenché quand le nombre de pairs LAN change, pour
/// que le runtime rafraîchisse l'événement `event.network`.
pub type OnChange = Arc<dyn Fn() + Send + Sync>;

/// Puits des pairs découverts sur le LAN : le runtime les enregistre comme
/// joignables (carnet d'adresses + ensemencement DHT best-effort).
#[async_trait::async_trait]
pub trait LanSink: Send + Sync {
    /// Prend en compte un pair Accord découvert sur le réseau local.
    async fn on_lan_peer(&self, pubkey: [u8; 32], addr: SocketAddr);
}

/// État partagé du LAN : ensemble des clés publiques découvertes (dédupliqué),
/// dont le cardinal est exposé par le statut réseau (`lan_peers`).
#[derive(Default)]
pub struct LanShared {
    peers: Mutex<HashSet<[u8; 32]>>,
}

impl LanShared {
    /// Nombre de pairs distincts actuellement découverts sur le LAN.
    pub fn count(&self) -> usize {
        self.peers.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Clés publiques des pairs actuellement découverts sur le LAN.
    ///
    /// Photographie : l'ensemble bouge au rythme des annonces. L'appairage s'en
    /// sert pour saluer les voisins déjà visibles, et complète par le rappel
    /// [`LanSink::on_lan_peer`] pour ceux qui apparaissent ensuite.
    pub fn peers(&self) -> Vec<[u8; 32]> {
        self.peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .copied()
            .collect()
    }

    /// Ajoute une clé ; rend vrai si elle était absente (transition).
    fn insert(&self, pubkey: [u8; 32]) -> bool {
        self.peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pubkey)
    }

    /// Retire une clé ; rend vrai si elle était présente (transition).
    fn remove(&self, pubkey: &[u8; 32]) -> bool {
        self.peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(pubkey)
    }
}

/// Nom d'instance mDNS d'un nœud : hex de sa clé publique (stable et unique,
/// caractères `[0-9a-f]` sans échappement).
pub fn instance_name(pubkey: &[u8; 32]) -> String {
    hex::encode(pubkey)
}

/// Nom d'hôte DNS annoncé (unique par nœud) : `<hex>.local.`.
pub fn host_name(pubkey: &[u8; 32]) -> String {
    format!("{}.local.", hex::encode(pubkey))
}

/// Propriétés TXT de l'annonce : la clé publique complète en hexadécimal.
pub fn txt_properties(pubkey: &[u8; 32]) -> Vec<(String, String)> {
    vec![(TXT_PUBKEY.to_string(), hex::encode(pubkey))]
}

/// Extrait un pair d'une annonce résolue : clé publique (hex 64 caractères) et
/// première adresse routable. Ignore soi-même, les entrées illisibles et les
/// adresses non routables (loopback, non spécifiée). Fonction pure et testable.
pub fn parse_peer(
    pk_hex: Option<&str>,
    self_pk: &[u8; 32],
    addrs: &[IpAddr],
    port: u16,
) -> Option<([u8; 32], SocketAddr)> {
    let pubkey = hex::decode::<32>(pk_hex?)?;
    if &pubkey == self_pk || port == 0 {
        return None;
    }
    let ip = addrs
        .iter()
        .copied()
        .find(|ip| !ip.is_loopback() && !ip.is_unspecified())?;
    Some((pubkey, SocketAddr::new(ip, port)))
}

/// Extrait la clé publique d'un nom complet mDNS `<hex>._accord._udp.local.`
/// (utilisé au retrait d'un service, qui ne porte que le nom). Fonction pure.
pub fn pubkey_from_fullname(fullname: &str) -> Option<[u8; 32]> {
    let instance = fullname
        .strip_suffix(SERVICE_TYPE)
        .and_then(|s| s.strip_suffix('.'))?;
    hex::decode::<32>(instance)
}

/// Événement LAN normalisé, extrait d'un événement mDNS brut. Découple la
/// consommation du type d'événement de la lib : la logique de traitement et de
/// supervision devient testable sans matériel réseau.
enum LanEvent {
    /// Un service Accord a été résolu (clé publique en TXT, adresses, port).
    Resolved {
        pk_hex: Option<String>,
        addrs: Vec<IpAddr>,
        port: u16,
    },
    /// Un service a été retiré (seul le nom complet est connu).
    Removed { fullname: String },
    /// Tout autre événement mDNS, sans intérêt ici.
    Other,
}

/// Marqueur : la session mDNS est morte (fil interne du démon terminé, canal
/// d'événements déconnecté). Signal de relance pour le superviseur.
struct DaemonDown;

/// Session mDNS active : source d'événements du LAN dont le fil sous-jacent peut
/// mourir hors de notre contrôle (panique interne de la lib). `next_event` rend
/// `Err(DaemonDown)` quand la session est morte.
#[async_trait::async_trait]
trait MdnsSession: Send {
    /// Prochain événement LAN, ou `Err` si le démon est mort (canal clos).
    async fn next_event(&mut self) -> Result<LanEvent, DaemonDown>;
    /// Retire l'annonce et arrête proprement le démon (arrêt demandé).
    fn shutdown(&self);
}

/// Fabrique de sessions mDNS : (re)crée un démon + annonce + parcours à la
/// demande, pour que le superviseur puisse relancer la découverte après panne.
trait MdnsBackend: Send {
    type Session: MdnsSession;
    /// Crée une session (démon + annonce + parcours). `None` si indisponible
    /// (raison journalisée par l'implémentation) ; le superviseur réessaiera.
    fn create(&self) -> Option<Self::Session>;
}

/// Politique de backoff exponentiel plafonné pour la relance du démon.
#[derive(Clone, Copy)]
struct Backoff {
    current: Duration,
    max: Duration,
}

impl Backoff {
    fn new(initial: Duration, max: Duration) -> Self {
        Self {
            current: initial,
            max,
        }
    }

    /// Rend le délai courant puis double (plafonné) pour la prochaine tentative.
    fn next_delay(&mut self) -> Duration {
        let delay = self.current;
        self.current = self
            .current
            .checked_mul(2)
            .unwrap_or(self.max)
            .min(self.max);
        delay
    }
}

/// Backend réel : recrée un [`mdns_sd::ServiceDaemon`], (ré)enregistre l'annonce
/// et ouvre un parcours à chaque appel de `create`.
struct RealBackend {
    self_pk: [u8; 32],
    local_ips: Vec<IpAddr>,
    port: u16,
}

impl MdnsBackend for RealBackend {
    type Session = RealSession;

    fn create(&self) -> Option<RealSession> {
        let daemon = match mdns_sd::ServiceDaemon::new() {
            Ok(d) => d,
            Err(e) => {
                tracing::debug!(erreur = %e, "mdns : démon indisponible, découverte en attente");
                return None;
            }
        };
        announce(&daemon, &self.self_pk, &self.local_ips, self.port);
        let browse = match daemon.browse(SERVICE_TYPE) {
            Ok(rx) => rx,
            Err(e) => {
                tracing::debug!(erreur = %e, "mdns : parcours impossible, découverte en attente");
                let _ = daemon.shutdown();
                return None;
            }
        };
        let fullname = format!("{}.{SERVICE_TYPE}", instance_name(&self.self_pk));
        Some(RealSession {
            daemon,
            browse,
            fullname,
        })
    }
}

/// Session réelle adossée à un démon mDNS vivant.
struct RealSession {
    daemon: mdns_sd::ServiceDaemon,
    browse: mdns_sd::Receiver<mdns_sd::ServiceEvent>,
    fullname: String,
}

#[async_trait::async_trait]
impl MdnsSession for RealSession {
    async fn next_event(&mut self) -> Result<LanEvent, DaemonDown> {
        match self.browse.recv_async().await {
            Ok(event) => Ok(normalize_event(event)),
            // Canal clos : le fil du démon s'est terminé (arrêt ou panique).
            Err(_) => Err(DaemonDown),
        }
    }

    fn shutdown(&self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

/// Convertit un événement mDNS brut en [`LanEvent`] (chemin réel uniquement).
fn normalize_event(event: mdns_sd::ServiceEvent) -> LanEvent {
    match event {
        mdns_sd::ServiceEvent::ServiceResolved(resolved) => LanEvent::Resolved {
            pk_hex: resolved.get_property_val_str(TXT_PUBKEY).map(str::to_owned),
            addrs: resolved
                .get_addresses()
                .iter()
                .map(|s| s.to_ip_addr())
                .collect(),
            port: resolved.get_port(),
        },
        mdns_sd::ServiceEvent::ServiceRemoved(_, fullname) => LanEvent::Removed { fullname },
        _ => LanEvent::Other,
    }
}

/// Lance l'annonce et la découverte mDNS en arrière-plan (non bloquant). La
/// tâche supervise le démon : si son fil meurt, elle le recrée avec backoff au
/// lieu de perdre la découverte en silence.
pub fn spawn(
    shared: Arc<LanShared>,
    self_pk: [u8; 32],
    local_ips: Vec<IpAddr>,
    port: u16,
    sink: Arc<dyn LanSink>,
    stop: watch::Receiver<bool>,
    on_change: OnChange,
) {
    if port == 0 {
        return;
    }
    let backend = RealBackend {
        self_pk,
        local_ips,
        port,
    };
    tokio::spawn(supervise(
        backend,
        shared,
        self_pk,
        sink,
        stop,
        on_change,
        Backoff::new(BACKOFF_INITIAL, BACKOFF_MAX),
    ));
}

/// Enregistre l'annonce du service local (best-effort : un échec journalise
/// sans interrompre la découverte des autres).
fn announce(daemon: &mdns_sd::ServiceDaemon, self_pk: &[u8; 32], local_ips: &[IpAddr], port: u16) {
    let name = instance_name(self_pk);
    let host = host_name(self_pk);
    let props = txt_properties(self_pk);
    let info =
        match mdns_sd::ServiceInfo::new(SERVICE_TYPE, &name, &host, local_ips, port, &props[..]) {
            Ok(info) => info.enable_addr_auto(),
            Err(e) => {
                tracing::debug!(erreur = %e, "mdns : annonce non construite");
                return;
            }
        };
    if let Err(e) = daemon.register(info) {
        tracing::debug!(erreur = %e, "mdns : annonce non enregistrée");
    }
}

/// Superviseur : maintient une session mDNS vivante jusqu'au signal d'arrêt. Si
/// la session meurt de façon inattendue (fil du démon terminé), journalise
/// l'incident et recrée le démon après un backoff (interruptible par l'arrêt).
async fn supervise<B>(
    backend: B,
    shared: Arc<LanShared>,
    self_pk: [u8; 32],
    sink: Arc<dyn LanSink>,
    mut stop: watch::Receiver<bool>,
    on_change: OnChange,
    mut backoff: Backoff,
) where
    B: MdnsBackend + 'static,
    B::Session: 'static,
{
    // Vrai après une première mort : la prochaine création est une relance (et
    // non le démarrage initial), à journaler comme reprise de la découverte.
    let mut relaunch = false;
    loop {
        if *stop.borrow() {
            return;
        }
        if let Some(session) = backend.create() {
            if relaunch {
                tracing::info!("mdns : démon recréé, découverte LAN relancée");
            }
            match consume(session, &shared, &self_pk, &sink, &mut stop, &on_change).await {
                ConsumeOutcome::Stopped => return,
                ConsumeOutcome::DaemonDied => {
                    tracing::warn!(
                        "mdns : le démon de découverte s'est arrêté de façon inattendue \
                         (panique interne probable sur une annonce LAN malformée) ; \
                         relance après backoff"
                    );
                    relaunch = true;
                }
            }
        }
        // Attente du backoff avant relance, interruptible par l'arrêt (aucune
        // relance une fois l'arrêt demandé).
        let delay = backoff.next_delay();
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            res = stop.changed() => {
                if res.is_err() || *stop.borrow() {
                    return;
                }
            }
        }
    }
}

/// Issue de la consommation d'une session : arrêt demandé, ou démon mort.
enum ConsumeOutcome {
    /// Arrêt demandé : la session a été fermée proprement.
    Stopped,
    /// Le démon est mort (canal clos) ; le superviseur doit le recréer.
    DaemonDied,
}

/// Consomme les événements d'une session jusqu'à l'arrêt ou la mort du démon :
/// enregistre/retire les pairs et notifie les transitions.
async fn consume<S: MdnsSession>(
    mut session: S,
    shared: &LanShared,
    self_pk: &[u8; 32],
    sink: &Arc<dyn LanSink>,
    stop: &mut watch::Receiver<bool>,
    on_change: &OnChange,
) -> ConsumeOutcome {
    loop {
        // On calcule d'abord l'issue de l'étape, hors emprunt de `session`, pour
        // pouvoir la fermer (`shutdown`) après la fin de `select!`.
        let outcome: Option<ConsumeOutcome> = tokio::select! {
            event = session.next_event() => match event {
                Ok(event) => {
                    apply_event(event, shared, self_pk, sink, on_change).await;
                    None
                }
                Err(DaemonDown) => Some(ConsumeOutcome::DaemonDied),
            },
            res = stop.changed() => {
                if res.is_err() || *stop.borrow() {
                    Some(ConsumeOutcome::Stopped)
                } else {
                    // Faux réveil du watch (valeur inchangée) : on continue.
                    None
                }
            }
        };
        match outcome {
            None => continue,
            Some(ConsumeOutcome::Stopped) => {
                session.shutdown();
                return ConsumeOutcome::Stopped;
            }
            Some(ConsumeOutcome::DaemonDied) => return ConsumeOutcome::DaemonDied,
        }
    }
}

/// Applique un événement LAN : enregistre/retire le pair et notifie la
/// transition (fonction du chemin de consommation, testable directement).
async fn apply_event(
    event: LanEvent,
    shared: &LanShared,
    self_pk: &[u8; 32],
    sink: &Arc<dyn LanSink>,
    on_change: &OnChange,
) {
    match event {
        LanEvent::Resolved {
            pk_hex,
            addrs,
            port,
        } => {
            if let Some((pubkey, addr)) = parse_peer(pk_hex.as_deref(), self_pk, &addrs, port) {
                if shared.insert(pubkey) {
                    on_change();
                }
                sink.on_lan_peer(pubkey, addr).await;
            }
        }
        LanEvent::Removed { fullname } => {
            if let Some(pubkey) = pubkey_from_fullname(&fullname) {
                if shared.remove(&pubkey) {
                    on_change();
                }
            }
        }
        LanEvent::Other => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn pk(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    /// Rappel `on_change` qui compte ses déclenchements.
    fn counting_on_change() -> (OnChange, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let hits = Arc::clone(&calls);
        let cb: OnChange = Arc::new(move || {
            hits.fetch_add(1, Ordering::SeqCst);
        });
        (cb, calls)
    }

    /// Puits LAN de test : enregistre les pairs reçus.
    #[derive(Default)]
    struct RecordingSink {
        peers: Mutex<Vec<([u8; 32], SocketAddr)>>,
    }

    #[async_trait::async_trait]
    impl LanSink for RecordingSink {
        async fn on_lan_peer(&self, pubkey: [u8; 32], addr: SocketAddr) {
            self.peers.lock().unwrap().push((pubkey, addr));
        }
    }

    /// Session factice : meurt une fois si `die_first`, sinon bloque
    /// indéfiniment (session « saine » et oisive). Compte les `shutdown`.
    struct FakeSession {
        die_first: bool,
        shutdowns: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl MdnsSession for FakeSession {
        async fn next_event(&mut self) -> Result<LanEvent, DaemonDown> {
            if self.die_first {
                self.die_first = false;
                Err(DaemonDown)
            } else {
                // Session saine et oisive : aucun événement, ne meurt pas.
                std::future::pending().await
            }
        }

        fn shutdown(&self) {
            self.shutdowns.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Backend factice : chaque `create` consomme un scénario (`true` = la
    /// session mourra aussitôt) ; scénarios épuisés → session saine.
    struct FakeBackend {
        scripts: Mutex<VecDeque<bool>>,
        creates: Arc<AtomicUsize>,
        shutdowns: Arc<AtomicUsize>,
    }

    impl MdnsBackend for FakeBackend {
        type Session = FakeSession;

        fn create(&self) -> Option<FakeSession> {
            self.creates.fetch_add(1, Ordering::SeqCst);
            let die_first = self.scripts.lock().unwrap().pop_front().unwrap_or(false);
            Some(FakeSession {
                die_first,
                shutdowns: Arc::clone(&self.shutdowns),
            })
        }
    }

    /// Attend qu'une condition devienne vraie en cédant la main, avec une borne
    /// dure pour ne jamais boucler indéfiniment (test déterministe, non flaky).
    async fn wait_until(mut pred: impl FnMut() -> bool) {
        for _ in 0..100_000 {
            if pred() {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("condition non atteinte à temps");
    }

    #[test]
    fn nom_instance_et_hote_reversibles() {
        let key = pk(0xAB);
        let name = instance_name(&key);
        assert_eq!(name.len(), 64);
        assert_eq!(host_name(&key), format!("{name}.local."));
        // Le nom complet se re-décode en clé publique.
        let fullname = format!("{name}.{SERVICE_TYPE}");
        assert_eq!(pubkey_from_fullname(&fullname), Some(key));
    }

    #[test]
    fn txt_porte_la_cle_publique() {
        let key = pk(0x01);
        let props = txt_properties(&key);
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].0, TXT_PUBKEY);
        assert_eq!(props[0].1, hex::encode(&key));
    }

    #[test]
    fn parse_peer_valide_et_ignore_soi_meme() {
        let me = pk(0x01);
        let other = pk(0x02);
        let other_hex = hex::encode(&other);
        let addrs = ["192.168.1.5".parse().unwrap()];

        // Pair valide : clé décodée + adresse routable.
        assert_eq!(
            parse_peer(Some(&other_hex), &me, &addrs, 48016),
            Some((other, "192.168.1.5:48016".parse().unwrap()))
        );
        // Soi-même : ignoré (évite de se compter comme pair).
        let me_hex = hex::encode(&me);
        assert!(parse_peer(Some(&me_hex), &me, &addrs, 48016).is_none());
        // Port nul : refusé.
        assert!(parse_peer(Some(&other_hex), &me, &addrs, 0).is_none());
        // Clé absente ou illisible : refusée.
        assert!(parse_peer(None, &me, &addrs, 48016).is_none());
        assert!(parse_peer(Some("pas-hex"), &me, &addrs, 48016).is_none());
    }

    #[test]
    fn parse_peer_ecarte_les_adresses_non_routables() {
        let me = pk(0x01);
        let other_hex = hex::encode(&pk(0x02));
        // Loopback et non spécifiée : écartées ; l'adresse LAN est retenue.
        let addrs = [
            "127.0.0.1".parse().unwrap(),
            "0.0.0.0".parse().unwrap(),
            "10.0.0.4".parse().unwrap(),
        ];
        assert_eq!(
            parse_peer(Some(&other_hex), &me, &addrs, 48016).map(|(_, a)| a),
            Some("10.0.0.4:48016".parse().unwrap())
        );
        // Aucune adresse routable : aucun pair.
        let only_loopback = ["127.0.0.1".parse().unwrap()];
        assert!(parse_peer(Some(&other_hex), &me, &only_loopback, 48016).is_none());
    }

    #[test]
    fn pubkey_from_fullname_rejette_les_noms_malformes() {
        assert!(pubkey_from_fullname("bonjour").is_none());
        // Bon suffixe mais instance non hexadécimale.
        assert!(pubkey_from_fullname(&format!("zzz.{SERVICE_TYPE}")).is_none());
        // Suffixe absent.
        let hexid = hex::encode(&pk(0x07));
        assert!(pubkey_from_fullname(&hexid).is_none());
    }

    #[test]
    fn backoff_double_puis_plafonne() {
        let mut b = Backoff::new(Duration::from_secs(1), Duration::from_secs(4));
        assert_eq!(b.next_delay(), Duration::from_secs(1));
        assert_eq!(b.next_delay(), Duration::from_secs(2));
        assert_eq!(b.next_delay(), Duration::from_secs(4));
        // Plafonné : ne dépasse jamais le maximum.
        assert_eq!(b.next_delay(), Duration::from_secs(4));
    }

    #[tokio::test]
    async fn apply_event_enregistre_puis_retire_le_pair() {
        let shared = LanShared::default();
        let sink_impl = Arc::new(RecordingSink::default());
        let sink: Arc<dyn LanSink> = Arc::clone(&sink_impl) as Arc<dyn LanSink>;
        let (on_change, changes) = counting_on_change();
        let me = pk(0x01);
        let other = pk(0x02);

        // Résolution d'un pair : compté, notifié, transmis au puits.
        apply_event(
            LanEvent::Resolved {
                pk_hex: Some(hex::encode(&other)),
                addrs: vec!["192.168.1.9".parse().unwrap()],
                port: 48016,
            },
            &shared,
            &me,
            &sink,
            &on_change,
        )
        .await;
        assert_eq!(shared.count(), 1);
        assert_eq!(changes.load(Ordering::SeqCst), 1);
        assert_eq!(
            sink_impl.peers.lock().unwrap().as_slice(),
            &[(other, "192.168.1.9:48016".parse().unwrap())]
        );

        // Retrait du même pair : décompté et notifié.
        apply_event(
            LanEvent::Removed {
                fullname: format!("{}.{SERVICE_TYPE}", instance_name(&other)),
            },
            &shared,
            &me,
            &sink,
            &on_change,
        )
        .await;
        assert_eq!(shared.count(), 0);
        assert_eq!(changes.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn demon_mort_est_relance_puis_arret_propre() {
        let creates = Arc::new(AtomicUsize::new(0));
        let shutdowns = Arc::new(AtomicUsize::new(0));
        // Première session : meurt aussitôt ; la suivante reste saine.
        let backend = FakeBackend {
            scripts: Mutex::new(VecDeque::from(vec![true])),
            creates: Arc::clone(&creates),
            shutdowns: Arc::clone(&shutdowns),
        };
        let (on_change, _changes) = counting_on_change();
        let (stop_tx, stop_rx) = watch::channel(false);
        // Backoff nul : la relance est immédiate (test rapide et déterministe).
        let handle = tokio::spawn(supervise(
            backend,
            Arc::new(LanShared::default()),
            pk(0x01),
            Arc::new(RecordingSink::default()) as Arc<dyn LanSink>,
            stop_rx,
            on_change,
            Backoff::new(Duration::ZERO, Duration::ZERO),
        ));

        // La mort de la 1re session doit provoquer une 2e création (relance).
        wait_until(|| creates.load(Ordering::SeqCst) >= 2).await;

        // Arrêt demandé : la session saine est fermée et la tâche se termine.
        stop_tx.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("le superviseur n'a pas terminé")
            .expect("le superviseur a paniqué");

        assert!(creates.load(Ordering::SeqCst) >= 2, "relance attendue");
        // Seule la session saine est fermée à l'arrêt ; la session morte ne
        // l'est pas (canal déjà clos), d'où exactement un `shutdown`.
        assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn arret_ferme_la_session_sans_relance() {
        let creates = Arc::new(AtomicUsize::new(0));
        let shutdowns = Arc::new(AtomicUsize::new(0));
        // Aucun scénario de mort : la 1re session est saine et le reste.
        let backend = FakeBackend {
            scripts: Mutex::new(VecDeque::new()),
            creates: Arc::clone(&creates),
            shutdowns: Arc::clone(&shutdowns),
        };
        let (on_change, _changes) = counting_on_change();
        let (stop_tx, stop_rx) = watch::channel(false);
        let handle = tokio::spawn(supervise(
            backend,
            Arc::new(LanShared::default()),
            pk(0x01),
            Arc::new(RecordingSink::default()) as Arc<dyn LanSink>,
            stop_rx,
            on_change,
            Backoff::new(Duration::ZERO, Duration::ZERO),
        ));

        wait_until(|| creates.load(Ordering::SeqCst) >= 1).await;
        stop_tx.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("le superviseur n'a pas terminé")
            .expect("le superviseur a paniqué");

        // Une seule session créée (pas de relance) et fermée proprement.
        assert_eq!(creates.load(Ordering::SeqCst), 1);
        assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    }
}
