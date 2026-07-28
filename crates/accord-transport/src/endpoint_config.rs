//! Configuration de l'endpoint transport : purement déclarative, aucune
//! logique. Extraite d'`endpoint.rs`, dont le seul `impl Endpoint` dépasse
//! déjà deux mille lignes — il n'y avait pas de couture propre à l'intérieur,
//! il y en avait une ici.

/// Configuration de l'endpoint.
#[derive(Debug, Clone, Copy)]
pub struct EndpointConfig {
    /// Difficulté PoW exigée des pairs.
    pub pow_bits: u32,
    /// Intervalle de keep-alive UDP (ms).
    pub keepalive_ms: u64,
    /// Inactivité avant fermeture de session (ms).
    pub idle_timeout_ms: u64,
    /// Seuil de HELLO/s au-delà duquel les cookies anti-DoS sont exigés.
    pub cookie_pressure_per_s: u32,
    /// Active le service de relais (SPEC §10) : ce nœud n'accepte d'acheminer du
    /// trafic pour des tiers que si ce drapeau est vrai. Le nœud ne l'active que
    /// lorsqu'il se sait publiquement joignable (hors périmètre ici). Faux par
    /// défaut : un nœud n'est jamais relais à son insu (limitation de la surface
    /// d'abus).
    pub relay_serving: bool,
    /// Capacités annoncées dans le handshake, ou `None` pour n'en annoncer
    /// aucune.
    ///
    /// Le champ occupe 4 octets après la signature du HELLO ; un pair dont la
    /// version est antérieure à son introduction (6.2) rejette tout octet
    /// excédentaire et ne peut alors plus établir la moindre session. C'est ce
    /// déploiement en deux temps — savoir lire d'abord, écrire ensuite — qui a
    /// permis d'allumer l'émission sans rupture.
    ///
    /// ⚠️ **Ce commentaire a dit « reste à `None` » plus longtemps qu'il ne
    /// fallait**, alors que le nœud posait déjà `Some(CAP_PQ_HYBRID)` — une
    /// contradiction qui a coûté une fausse alerte de rupture filaire lors de
    /// la fusion du jalon 2. Le plancher réel du parc n'est plus 6.2 mais 6.3 :
    /// le jour de rupture de la 7.0 (clés d'appareil) a déjà coupé tout pair en
    /// 6.2 ou antérieur, qui voit un ami basculé comme un inconnu et meurt sur
    /// `PeerIdentityMismatch` (`docs/MULTI_DEVICE.md` §3.2.1). Émettre
    /// n'atteint donc plus personne que ce jour-là n'ait déjà coupé.
    ///
    /// La valeur par défaut reste `None` ici : c'est au nœud de décider ce
    /// qu'il annonce, pas au transport de l'imposer à tout appelant.
    pub capabilities: Option<u32>,
    /// Refuser toute session dont la clé ne dérive PAS aussi d'un secret
    /// ML-KEM (réglage avancé, lot 2.D). Faux par défaut : la politique
    /// ordinaire est « accepter les deux, préférer l'hybride », parce qu'un
    /// refus généralisé couperait les amis restés sur une version antérieure.
    ///
    /// Vrai, le handshake est mené jusqu'au bout — il faut la signature pour
    /// savoir de quoi la clé dérive vraiment — puis la session est écartée
    /// avant installation. C'est une politique LOCALE : le pair n'apprend rien
    /// du refus qu'un pair injoignable ne lui apprendrait pas déjà.
    ///
    /// Modifiable à chaud par [`Endpoint::set_require_post_quantum`] : ce
    /// réglage-là ne doit pas attendre un redémarrage pour prendre effet.
    pub require_post_quantum: bool,
}

impl Default for EndpointConfig {
    fn default() -> Self {
        Self {
            pow_bits: accord_proto::limits::IDENTITY_POW_BITS,
            keepalive_ms: 25_000,
            idle_timeout_ms: 120_000,
            cookie_pressure_per_s: 64,
            relay_serving: false,
            capabilities: None,
            require_post_quantum: false,
        }
    }
}

// --- Codes de refus d'ouverture de circuit relais (canal RELAY, SPEC §10) ---
/// Le nœud sollicité n'assure pas le service de relais (`relay_serving == false`).
pub(crate) const REJECT_NOT_RELAY: u8 = 0x01;
/// Le relais n'a aucune session active avec la cible demandée.
pub(crate) const REJECT_NO_TARGET: u8 = 0x02;
/// La table de circuits du relais est pleine (`MAX_CIRCUITS` atteint).
pub(crate) const REJECT_FULL: u8 = 0x03;

use std::net::SocketAddr;

/// Photographie d'une session établie, exposée à la couche nœud pour le
/// diagnostic de connectivité par pair (D4/D35) : lien direct ou tunnelé,
/// fraîcheur du dernier trafic entrant, latence estimée. Aucune donnée
/// applicative ni clé n'y figure.
#[derive(Debug, Clone, Copy)]
pub struct SessionView {
    /// Clé publique Ed25519 du pair (session authentifiée).
    pub peer_static: [u8; 32],
    /// Adresse de transport : celle du pair en direct, celle du RELAIS pour
    /// une session tunnelée.
    pub addr: SocketAddr,
    /// `Some(circuit)` si la session transite par un circuit relais.
    pub relay_circuit: Option<u32>,
    /// Horodatage (ms, horloge du nœud) du dernier trafic entrant.
    pub last_recv_ms: u64,
    /// Dernier aller-retour keep-alive mesuré (ms), si un cycle a abouti.
    pub last_rtt_ms: Option<u64>,
    /// Capacités authentifiées du pair, telles que liées au transcript du
    /// handshake. 0 si le pair n'en annonce aucune.
    pub peer_capabilities: u32,
    /// Vrai si la session a été négociée en hybride post-quantique : sa clé
    /// dérive du X25519 **et** d'un secret ML-KEM. Faux en session classique.
    pub is_post_quantum: bool,
}

/// Nombre de salves de HELLO simultanées émises lors d'un poinçonnage
/// coordonné (SPEC §11.2 : « 5 tentatives »).
pub(crate) const PUNCH_ATTEMPTS: u32 = 5;
/// Intervalle entre deux salves de poinçonnage (SPEC §11.2 : « 200 ms
/// d'intervalle »).
pub(crate) const PUNCH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

/// Plafond du nombre de circuits relais dont ce nœud est extrémité CLIENTE
/// (miroir de [`crate::relay::MAX_CIRCUITS`] côté serveur). Borne l'empreinte
/// mémoire de `client_circuits` — en particulier les circuits ouverts par des
/// HELLO tunnelés entrants, insérés AVANT tout PoW/rate-limit (FAILLE C).
pub(crate) const MAX_CLIENT_CIRCUITS: usize = crate::relay::MAX_CIRCUITS;

/// Capacité (rafale) du seau de messages de contrôle changeant l'état par
/// session : couvre l'annonce initiale, quelques ré-annonces et les
/// observations d'adresse d'un cycle de présence sans jamais bloquer un pair
/// honnête.
pub(crate) const CTRL_MSG_BURST: f64 = 8.0;
/// Recharge du seau de contrôle (messages/s) : au-delà, les messages
/// excédentaires sont ignorés silencieusement. À 1/s, un pair hostile qui
/// inonde est ramené à un filet négligeable (≈ 1 insertion de table/s),
/// trivialement absorbé, tout en laissant passer le trafic légitime (une
/// poignée de messages par minute au plus).
pub(crate) const CTRL_MSG_REFILL_PER_S: f64 = 1.0;
