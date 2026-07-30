//! Garde-fous de décodage et constantes protocolaires (SPEC §13).

/// Version courante du protocole filaire.
pub const PROTOCOL_VERSION: u8 = 1;

/// Borne par défaut du nombre d'éléments d'une `list<T>`.
///
/// Chaque champ déclare la sienne au décodage ([`crate::wire::Reader::list`]) et
/// c'est celle-là qui fait foi : presque toutes sont bien plus strictes (4
/// adresses de nœud, 8 appareils, 10 pièces jointes…), et une seule est plus
/// large — les feuilles d'un manifest, dont le nombre dérive de
/// [`MAX_FILE_SIZE`] / [`FILE_BLOCK_SIZE`] et vaut jusqu'à 8192.
pub const MAX_LIST: usize = 4096;

/// Taille maximale d'un `lbytes` au décodage.
pub const MAX_LBYTES: usize = 16 * 1024 * 1024;

/// MTU applicative UDP : un paquet non-TCP ne dépasse jamais cette taille.
pub const UDP_MTU: usize = 1200;

/// Taille maximale d'un frame TCP (préfixe u32).
pub const MAX_TCP_FRAME: usize = 1024 * 1024;

/// Taille maximale de la valeur d'un record DHT.
pub const MAX_DHT_VALUE: usize = 8 * 1024;

/// Longueur maximale d'un message texte (en octets UTF-8).
pub const MAX_TEXT_BYTES: usize = 4 * 8000;

/// Taille maximale d'une pièce jointe.
pub const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024 * 1024;

/// Taille d'un bloc de fichier.
pub const FILE_BLOCK_SIZE: usize = 256 * 1024;

/// Profondeur maximale d'un arbre de Merkle.
pub const MERKLE_MAX_DEPTH: usize = 24;

/// Participants simultanés maximum d'un salon vocal.
pub const VOICE_MAX_PARTICIPANTS: usize = 10;

/// Fenêtre anti-rejeu du handshake (millisecondes).
pub const HANDSHAKE_MAX_SKEW_MS: u64 = 90_000;

/// Capacité : le pair comprend les identités d'appareil (multi-appareil).
pub const CAP_DEVICE_KEYS: u32 = 1 << 0;

/// Capacité : le pair sait mener un handshake hybride post-quantique.
pub const CAP_PQ_HYBRID: u32 = 1 << 1;

/// Capacité : le pair sait recevoir de la vidéo de plusieurs émetteurs.
pub const CAP_GROUP_VIDEO_N: u32 = 1 << 2;

/// Bits de capacité connus de cette version. Tout bit hors de ce masque est
/// **ignoré** au lieu d'être une erreur : c'est ce qui permet d'ajouter des
/// capacités sans casser les pairs plus anciens.
pub const CAP_KNOWN: u32 = CAP_DEVICE_KEYS | CAP_PQ_HYBRID | CAP_GROUP_VIDEO_N;

/// Taille d'une clé d'encapsulation ML-KEM-512 (FIPS 203, §8, jeu de
/// paramètres de catégorie 1). Portée par le HELLO d'un initiateur hybride.
///
/// 🔒 **Borne de décodage, pas d'usage.** La taille est fixée par le jeu de
/// paramètres du protocole : le décodeur lit exactement ce nombre d'octets et
/// n'alloue jamais d'après une longueur venue du réseau. Le handshake précède
/// l'établissement de session — il n'est donc pas authentifié — et un préfixe
/// de longueur choisi par l'émetteur y ouvrirait une voie d'épuisement mémoire.
/// Un autre jeu de paramètres prendra un bit de capacité neuf, jamais celui-ci.
pub const MLKEM512_EK_BYTES: usize = 800;

/// Taille d'un chiffré d'encapsulation ML-KEM-512. Porté par le WELCOME d'un
/// répondeur hybride. Même règle de bornage que [`MLKEM512_EK_BYTES`].
pub const MLKEM512_CT_BYTES: usize = 768;

/// Nombre de trames avant re-keying obligatoire d'une session.
pub const REKEY_FRAME_LIMIT: u64 = 1_000_000;

/// Âge maximal d'une clé de session avant re-keying (secondes).
pub const REKEY_MAX_AGE_S: u64 = 24 * 3600;

/// Bits de tête à zéro exigés par la preuve de travail d'identité.
pub const IDENTITY_POW_BITS: u32 = 16;

/// Paramètre Kademlia k (taille de bucket et facteur de réplication).
pub const DHT_K: usize = 20;

/// Parallélisme α des lookups Kademlia.
pub const DHT_ALPHA: usize = 3;

/// Timeout d'un RPC DHT (millisecondes) avant retransmission.
pub const DHT_RPC_TIMEOUT_MS: u64 = 2_000;

/// Nombre de retransmissions d'un RPC DHT après le premier envoi.
pub const DHT_RPC_RETRIES: u32 = 2;

/// Expiration maximale d'un record DHT (secondes) : 7 jours.
pub const DHT_MAX_EXPIRY_S: u32 = 7 * 24 * 3600;

/// Adresses maximales portées par un NodeInfo.
pub const MAX_NODE_ADDRS: usize = 4;

/// Candidats d'adresse maximum portés par une demande ou une réponse de
/// poinçonnage coordonné (SPEC §11.2). Borne stricte anti-abus : un pair,
/// même authentifié, ne peut pas faire émettre des HELLO vers plus de
/// 8 cibles par échange.
pub const MAX_PUNCH_CANDIDATES: usize = 8;
