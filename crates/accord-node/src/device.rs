//! Migration d'un profil vers le modèle compte/appareil (jalon 1, lot 1.B).
//!
//! Au premier démarrage sur un profil existant, la graine actuelle devient la
//! **racine du compte** — le code ami, le profil et toutes les amitiés
//! continuent de pointer sur la même clé publique, donc rien ne change pour
//! les correspondants — et une **clé d'appareil neuve et distincte** est
//! générée pour cette machine.
//!
//! 🔒 La distinction est tout l'objet du jalon. Si la graine de compte servait
//! aussi d'appareil, deux machines restaurées depuis la même phrase de
//! récupération partageraient leur identité de transport et s'évinceraient
//! l'une l'autre à chaque ami (invariant « au plus une session directe par
//! identité »). Voir `docs/MULTI_DEVICE.md` §1.
//!
//! Le module porte aussi la **phase 1** du lot 1.C : construire la liste
//! d'appareils de ce compte et la publier dans la DHT. Le transport, lui,
//! continue de **présenter** la clé de compte — c'est la moitié « savoir
//! lire » du déploiement en deux temps (voir `docs/MULTI_DEVICE.md` §3.2.1).
//! Présenter la clé d'appareil avant que le parc sache la résoudre ferait de
//! nous un inconnu pour chacun de nos amis.

use accord_core::db::LocalDevice;
use accord_core::Db;
use accord_crypto::{
    device_list_key, verify_device_list_with_pow_bits, version_for, AccountIdentity, DeviceIdentity,
};
use accord_proto::device::{DeviceEntry, DeviceList};
use accord_proto::limits::IDENTITY_POW_BITS;
use accord_proto::types::{DhtRecord, RecordKind};
use accord_proto::{WireDecode, WireEncode};

use crate::error::NodeError;

/// Nom par défaut d'un appareil migré, avant que l'utilisateur ne le renomme.
const DEFAULT_DEVICE_NAME: &str = "Cet appareil";

/// Durée de validité d'une liste publiée (secondes) — 24 h.
///
/// Compromis assumé entre deux coûts opposés : trop courte, chaque
/// correspondant re-résout sans cesse ; trop longue, un appareil révoqué
/// survit d'autant sur des listes périmées (§3.3). Un jour place la révocation
/// dans le même ordre de grandeur qu'un changement de mot de passe ailleurs.
pub const DEVICE_LIST_VALID_S: u32 = 24 * 3600;

/// Garantit qu'une identité d'appareil existe pour cette machine, et la rend.
///
/// Idempotent : au deuxième démarrage, l'appareil persisté est simplement
/// rechargé. Générer une nouvelle clé à chaque lancement ferait de chaque
/// redémarrage un appareil de plus aux yeux des amis.
pub fn ensure_local_device(db: &Db) -> Result<DeviceIdentity, NodeError> {
    if let Some(stored) = db.local_device()? {
        return Ok(DeviceIdentity::from_seed(stored.seed));
    }
    let device = DeviceIdentity::generate();
    db.set_local_device(&LocalDevice {
        seed: *device.seed(),
        pow_nonce: device.pow_nonce(),
        name: DEFAULT_DEVICE_NAME.to_string(),
    })?;
    tracing::info!("identité d'appareil créée pour cette machine");
    Ok(device)
}

/// Construit la liste d'appareils de ce compte, signée par la racine.
///
/// Un seul appareil pour l'instant — celui de cette machine. L'appairage (lot
/// 1.D) en ajoutera d'autres ; la forme de la liste, elle, ne changera pas.
///
/// ⚠️ La version vient de l'horodatage, pas d'un compteur stocké. Après une
/// restauration depuis la phrase de récupération, un compteur repartirait à 1
/// et les pairs détenant une version supérieure **ignoreraient** la nouvelle
/// liste — l'utilisateur resterait enfermé dehors de son propre compte.
pub fn build_device_list(
    account: &AccountIdentity,
    device: &DeviceIdentity,
    name: &str,
    now_ms: u64,
    flags: u32,
) -> DeviceList {
    build_device_list_with_root(account.identity(), device, name, now_ms, flags)
}

/// [`build_device_list`] pour un appelant qui ne possède pas son identité
/// (`Arc<Identity>` partagé) — voir `accord_crypto::sign_device_list_with_root`.
pub fn build_device_list_with_root(
    root: &accord_crypto::Identity,
    device: &DeviceIdentity,
    name: &str,
    now_ms: u64,
    flags: u32,
) -> DeviceList {
    let mut list = DeviceList {
        account: root.public_key(),
        version: version_for(now_ms),
        issued_ms: now_ms,
        valid_for_s: DEVICE_LIST_VALID_S,
        devices: vec![DeviceEntry {
            pubkey: device.public_key(),
            pow_nonce: device.pow_nonce(),
            name: name.to_string(),
            added_ms: now_ms,
            flags,
        }],
        revoked: Vec::new(),
        sig: [0u8; 64],
    };
    accord_crypto::sign_device_list_with_root(root, &mut list);
    list
}

/// Drapeaux à porter par l'entrée de l'appareil local, d'après la clé que son
/// transport présente réellement.
///
/// 🔒 Déduit, jamais recopié depuis la configuration : c'est la clé effective
/// qui fait foi. Un drapeau qui affirmerait un basculement que le transport
/// n'a pas fait dirigerait tous les messages du compte vers une clé que
/// personne n'écoute.
pub fn local_device_flags(transport_pub: &[u8; 32], device_pub: &[u8; 32]) -> u32 {
    if transport_pub == device_pub {
        accord_proto::device::DEVICE_FLAG_TRANSPORT_KEY
    } else {
        0
    }
}

/// Emballe une liste d'appareils en record DHT signé, prêt à publier.
///
/// 🔒 Le publieur **est** le compte, et la clé DHT dérive de lui : c'est ce
/// double ancrage que `RecordStore::validate` vérifie, et qui empêche de
/// publier sa propre liste à l'adresse de quelqu'un d'autre.
pub fn device_list_record(account: &AccountIdentity, list: &DeviceList, now_ms: u64) -> DhtRecord {
    device_list_record_with_root(account.identity(), list, now_ms)
}

/// [`device_list_record`] pour un appelant qui ne possède pas son identité.
pub fn device_list_record_with_root(
    root: &accord_crypto::Identity,
    list: &DeviceList,
    now_ms: u64,
) -> DhtRecord {
    let mut w = accord_proto::Writer::new();
    list.encode(&mut w);
    let mut record = DhtRecord {
        key: device_list_key(&root.public_key()),
        kind: RecordKind::DeviceList,
        value: w.into_bytes(),
        publisher: root.public_key(),
        timestamp_ms: now_ms,
        expiry_s: DEVICE_LIST_VALID_S,
        sig: [0u8; 64],
    };
    record.sig = root.sign(&record.signable_bytes());
    record
}

/// Vérifie un record DEVICE_LIST censé venir de `account`, et rend la liste.
///
/// 🔒 L'ordre des contrôles est délibéré : nature, publieur et ancrage de clé
/// se règlent en quelques comparaisons, **avant** le décodage et les
/// vérifications de signature. Un record arrivant de la DHT vient d'inconnus ;
/// faire l'inverse offrirait à qui veut nous inonder un levier de déni de
/// service à bon marché.
pub fn verify_device_list_record(
    account: &[u8; 32],
    record: &DhtRecord,
    known_version: u64,
) -> Result<DeviceList, NodeError> {
    verify_device_list_record_with_pow_bits(account, record, known_version, IDENTITY_POW_BITS)
}

/// [`verify_device_list_record`] à une difficulté de preuve de travail
/// explicite. Réservé aux tests — voir `accord_crypto::verify_device_list`.
pub fn verify_device_list_record_with_pow_bits(
    account: &[u8; 32],
    record: &DhtRecord,
    known_version: u64,
    pow_bits: u32,
) -> Result<DeviceList, NodeError> {
    if record.kind != RecordKind::DeviceList {
        return Err(NodeError::Invalid("record de nature inattendue"));
    }
    if record.publisher != *account {
        return Err(NodeError::Invalid("liste d'appareils non auto-publiée"));
    }
    if record.key != device_list_key(account) {
        return Err(NodeError::Invalid("liste d'appareils à une clé étrangère"));
    }
    let mut r = accord_proto::Reader::new(&record.value);
    let list = DeviceList::decode(&mut r)
        .map_err(|_| NodeError::Invalid("liste d'appareils illisible"))?;
    r.finish()
        .map_err(|_| NodeError::Invalid("octets excédentaires après la liste"))?;
    verify_device_list_with_pow_bits(&list, account, known_version, pow_bits)
        .map_err(|_| NodeError::Invalid("liste d'appareils refusée"))?;
    Ok(list)
}

/// Appareils par lesquels joindre `account`, d'après la liste en cache.
///
/// Rend une liste vide quand rien n'est connu **ou que le cache est périmé** :
/// l'appelant doit alors rafraîchir avant de faire confiance. Servir une liste
/// périmée ferait survivre un appareil révoqué aussi longtemps qu'elle (§3.3).
pub fn cached_devices_for(db: &Db, account: &[u8; 32], now_ms: u64) -> Vec<[u8; 32]> {
    cached_list_for(db, account, now_ms)
        .map(|list| {
            list.devices
                .iter()
                .map(|d| d.pubkey)
                .filter(|pk| list.authorises(pk))
                .collect()
        })
        .unwrap_or_default()
}

/// Vrai si l'on détient pour `account` une liste lisible et encore fraîche.
///
/// Distinct de « la liste contient des appareils » : une liste fraîche dont
/// tous les appareils sont révoqués reste une réponse valable, et la relever
/// encore serait du trafic pour rien — la DHT rendrait le même record, dont la
/// version serait alors refusée à chaque passe.
pub fn has_fresh_list(db: &Db, account: &[u8; 32], now_ms: u64) -> bool {
    cached_list_for(db, account, now_ms).is_some()
}

/// Liste d'appareils en cache pour `account`, si elle est lisible et fraîche.
fn cached_list_for(db: &Db, account: &[u8; 32], now_ms: u64) -> Option<DeviceList> {
    let cached = db.device_list(account).ok()??;
    let mut r = accord_proto::Reader::new(&cached.encoded);
    let list = DeviceList::decode(&mut r).ok()?;
    list.is_fresh(now_ms).then_some(list)
}

/// Compte auquel appartient la clé de transport `static_pub`, s'il en est un
/// que l'on connaît.
///
/// Deux cas se confondent volontairement pour l'appelant :
/// - la clé **est** celle d'un compte ami — c'est le cas de tout le parc
///   actuel, où l'identité de transport est encore l'identité de compte ;
/// - la clé est celle d'un **appareil** listé dans la liste fraîche d'un ami.
///
/// C'est la moitié « savoir lire » du basculement (voir `docs/MULTI_DEVICE.md`
/// §3.2.1). Tant que le parc présente sa clé de compte, seul le premier cas se
/// produit ; le second doit néanmoins être déployé **avant** que quiconque
/// commence à présenter une clé d'appareil, sinon les premiers à basculer
/// deviendraient des inconnus pour tous les autres.
///
/// Une liste périmée ne rattache rien : voir [`cached_devices_for`].
pub fn account_for_static(
    db: &Db,
    friends: &[[u8; 32]],
    static_pub: &[u8; 32],
    now_ms: u64,
) -> Option<[u8; 32]> {
    if friends.contains(static_pub) {
        return Some(*static_pub);
    }
    friends
        .iter()
        .find(|account| cached_devices_for(db, account, now_ms).contains(static_pub))
        .copied()
}

/// Clés de transport à atteindre pour livrer à `account`.
///
/// La règle tient en une phrase : **un appareil est joignable à sa propre clé
/// s'il annonce la présenter, et à la clé du compte sinon.**
///
/// Concrètement :
/// - aucune liste fraîche connue → la clé de compte, seule ;
/// - liste où aucun appareil ne porte [`DEVICE_FLAG_TRANSPORT_KEY`] → la clé de
///   compte, seule (tout le parc pendant la phase 1) ;
/// - liste entièrement basculée → une clé par appareil ;
/// - liste **mixte** → les appareils basculés, **plus** la clé de compte pour
///   ceux qui ne le sont pas.
///
/// 🔒 Le dernier cas est celui qui existera réellement pendant des semaines, et
/// c'est celui qu'on rate en le simplifiant. Ne garder que les appareils
/// basculés couperait les autres ; ajouter systématiquement la clé de compte
/// ferait déposer, à jamais, un message en boîte pour un destinataire qui
/// n'écoute plus. Les appareils non basculés se confondent tous en une seule
/// cible parce qu'ils présentent tous la même clé — et s'évincent d'ailleurs
/// mutuellement du transport, ce qui est précisément le blocage que ce jalon
/// lève.
///
/// ⚠️ Un compte à N appareils multiplie le trafic par N pour un message
/// direct. Négligeable pour du texte ; **inacceptable pour la voix et la
/// vidéo**, qui restent mono-appareil (§5).
pub fn delivery_targets(db: &Db, account: &[u8; 32], now_ms: u64) -> Vec<[u8; 32]> {
    let Some(list) = cached_list_for(db, account, now_ms) else {
        return vec![*account];
    };
    let mut targets = Vec::with_capacity(list.devices.len() + 1);
    let mut account_needed = list.devices.is_empty();
    for device in &list.devices {
        if !list.authorises(&device.pubkey) {
            continue;
        }
        if device.presents_own_key() {
            if !targets.contains(&device.pubkey) {
                targets.push(device.pubkey);
            }
        } else {
            account_needed = true;
        }
    }
    // La clé de compte passe en tête : c'est celle du parc actuel, donc celle
    // qui aboutit dans l'immense majorité des cas.
    if account_needed && !targets.contains(account) {
        targets.insert(0, *account);
    }
    if targets.is_empty() {
        // Liste fraîche mais dont chaque appareil est révoqué : on ne peut pas
        // conclure « injoignable ». La clé de compte reste le dernier recours.
        return vec![*account];
    }
    targets
}

#[cfg(test)]
mod tests {
    use super::*;
    use accord_crypto::Identity;

    /// Drapeaux d'un appareil qui présente sa propre clé — l'état d'après le
    /// basculement, et celui que la plupart de ces tests supposent.
    const TRANSPORT: u32 = accord_proto::device::DEVICE_FLAG_TRANSPORT_KEY;

    fn db() -> Db {
        Db::open_in_memory(&[4u8; 32]).expect("base en mémoire")
    }

    #[test]
    fn a_device_is_created_on_first_start() {
        let db = db();
        assert!(db.local_device().unwrap().is_none());
        let device = ensure_local_device(&db).unwrap();
        let stored = db.local_device().unwrap().expect("appareil persisté");
        assert_eq!(stored.seed, *device.seed());
        assert_eq!(stored.pow_nonce, device.pow_nonce());
    }

    #[test]
    fn restarting_reuses_the_same_device() {
        // Une clé neuve à chaque lancement ferait de chaque redémarrage un
        // appareil de plus dans la liste, jusqu'à en dépasser la borne.
        let db = db();
        let first = ensure_local_device(&db).unwrap().public_key();
        for _ in 0..5 {
            assert_eq!(ensure_local_device(&db).unwrap().public_key(), first);
        }
    }

    #[test]
    fn the_device_key_differs_from_the_account_key() {
        // 🔒 Le cœur du jalon : confondre les deux ramènerait l'éviction
        // mutuelle que tout ce chantier existe pour supprimer.
        let db = db();
        let account = Identity::generate_with_pow_bits(4);
        let device = ensure_local_device(&db).unwrap();
        assert_ne!(device.public_key(), account.public_key());
        assert_ne!(device.seed(), account.seed());
    }

    #[test]
    fn two_machines_of_the_same_account_get_distinct_devices() {
        // Deux profils distincts (deux machines) migrés depuis la même phrase
        // de récupération : même compte, appareils différents.
        let a = ensure_local_device(&db()).unwrap();
        let b = ensure_local_device(&db()).unwrap();
        assert_ne!(a.public_key(), b.public_key());
    }

    #[test]
    fn the_generated_device_carries_a_valid_proof_of_work() {
        // Sans preuve de travail, une clé d'appareil se fabrique en masse : la
        // liste d'appareils d'un compte deviendrait un vecteur d'inondation.
        let device = ensure_local_device(&db()).unwrap();
        assert!(accord_crypto::verify_pow(
            &device.public_key(),
            device.pow_nonce(),
            accord_proto::limits::IDENTITY_POW_BITS,
        ));
    }

    #[test]
    fn la_liste_publiee_autorise_lappareil_local_et_pas_le_compte() {
        // 🔒 La clé de compte n'est PAS un appareil. Si elle l'était, deux
        // machines restaurées depuis la même phrase se retrouveraient avec la
        // même identité de transport — exactement ce que le jalon corrige.
        let account = AccountIdentity::from_identity(Identity::generate_with_pow_bits(1));
        let device = DeviceIdentity::generate_with_pow_bits(1);
        let list = build_device_list(&account, &device, "Portable", 1_700_000_000_000, TRANSPORT);

        assert!(list.authorises(&device.public_key()));
        assert!(!list.authorises(&account.public_key()));
    }

    #[test]
    fn la_liste_publiee_se_verifie_avec_la_cle_du_compte() {
        let account = AccountIdentity::from_identity(Identity::generate_with_pow_bits(1));
        let device = DeviceIdentity::generate_with_pow_bits(1);
        let list = build_device_list(&account, &device, "Portable", 1_700_000_000_000, TRANSPORT);

        // Difficulté de test : la PoW réelle rendrait ce test interminable.
        // On passe par la VRAIE vérification, pas par une copie.
        assert!(
            accord_crypto::verify_device_list_with_pow_bits(&list, &account.public_key(), 0, 1)
                .is_ok(),
            "une liste fraîchement signée doit se vérifier"
        );
    }

    #[test]
    fn le_record_publie_est_ancre_sur_le_compte() {
        // Sans cet ancrage, la liste serait publiable à l'adresse d'un autre
        // compte : c'est `RecordStore::validate` qui le refuse, et ce test dit
        // que le record qu'on fabrique lui convient.
        let account = AccountIdentity::from_identity(Identity::generate_with_pow_bits(1));
        let device = DeviceIdentity::generate_with_pow_bits(1);
        let now = 1_700_000_000_000;
        let list = build_device_list(&account, &device, "Portable", now, TRANSPORT);
        let record = device_list_record(&account, &list, now);

        assert_eq!(record.kind, RecordKind::DeviceList);
        assert_eq!(record.publisher, account.public_key());
        assert_eq!(record.key, device_list_key(&account.public_key()));
        assert!(accord_dht::RecordStore::validate(&record).is_ok());
    }

    #[test]
    fn le_record_publie_se_redecode_en_la_meme_liste() {
        let account = AccountIdentity::from_identity(Identity::generate_with_pow_bits(1));
        let device = DeviceIdentity::generate_with_pow_bits(1);
        let now = 1_700_000_000_000;
        let list = build_device_list(&account, &device, "Portable", now, TRANSPORT);
        let record = device_list_record(&account, &list, now);

        let mut r = accord_proto::Reader::new(&record.value);
        let relue = DeviceList::decode(&mut r).expect("liste relisible");
        assert_eq!(relue, list);
    }

    #[test]
    fn deux_emissions_successives_ont_des_versions_croissantes() {
        // La version vient de l'horloge : une réémission plus tardive doit
        // toujours dépasser la précédente, sinon les pairs l'ignoreraient.
        let account = AccountIdentity::from_identity(Identity::generate_with_pow_bits(1));
        let device = DeviceIdentity::generate_with_pow_bits(1);
        let premiere =
            build_device_list(&account, &device, "Portable", 1_700_000_000_000, TRANSPORT);
        let seconde =
            build_device_list(&account, &device, "Portable", 1_700_000_060_000, TRANSPORT);

        assert!(seconde.version > premiere.version);
    }

    /// Compte, appareil et record cohérents, à la difficulté de test.
    fn published(now: u64) -> (AccountIdentity, DeviceIdentity, DhtRecord) {
        let account = AccountIdentity::from_identity(Identity::generate_with_pow_bits(1));
        let device = DeviceIdentity::generate_with_pow_bits(1);
        let list = build_device_list(&account, &device, "Portable", now, TRANSPORT);
        let record = device_list_record(&account, &list, now);
        (account, device, record)
    }

    fn verifier(
        account: &[u8; 32],
        record: &DhtRecord,
        known: u64,
    ) -> Result<DeviceList, NodeError> {
        verify_device_list_record_with_pow_bits(account, record, known, 1)
    }

    #[test]
    fn un_record_bien_forme_rend_la_liste() {
        let now = 1_700_000_000_000;
        let (account, device, record) = published(now);
        let list = verifier(&account.public_key(), &record, 0).expect("liste acceptée");
        assert!(list.authorises(&device.public_key()));
    }

    #[test]
    fn un_record_dun_autre_compte_est_refuse() {
        let now = 1_700_000_000_000;
        let (_, _, record) = published(now);
        let etranger = Identity::generate_with_pow_bits(1).public_key();
        assert!(verifier(&etranger, &record, 0).is_err());
    }

    #[test]
    fn un_record_deplace_a_une_autre_cle_est_refuse() {
        // Le publieur et la signature sont authentiques ; seule la clé DHT ment.
        // Sans ce contrôle, la liste d'Alice servirait de réponse à une
        // recherche portant sur quelqu'un d'autre.
        let now = 1_700_000_000_000;
        let (account, _, mut record) = published(now);
        record.key = [9u8; 32];
        record.sig = account.identity().sign(&record.signable_bytes());
        assert!(verifier(&account.public_key(), &record, 0).is_err());
    }

    #[test]
    fn une_version_deja_connue_est_refusee() {
        // Rejeu d'une liste ancienne : c'est ce contrôle qui empêche de
        // ressusciter un appareil révoqué depuis.
        let now = 1_700_000_000_000;
        let (account, _, record) = published(now);
        let deja = verifier(&account.public_key(), &record, 0).unwrap().version;
        assert!(verifier(&account.public_key(), &record, deja).is_err());
    }

    #[test]
    fn des_octets_en_trop_apres_la_liste_sont_refuses() {
        let now = 1_700_000_000_000;
        let (account, _, mut record) = published(now);
        record.value.push(0);
        record.sig = account.identity().sign(&record.signable_bytes());
        assert!(verifier(&account.public_key(), &record, 0).is_err());
    }

    #[test]
    fn le_cache_rend_les_appareils_tant_que_la_liste_est_fraiche() {
        let now = 1_700_000_000_000;
        let db = db();
        let (account, device, record) = published(now);
        db.cache_device_list(&accord_core::db::CachedDeviceList {
            account: account.public_key(),
            version: version_for(now),
            encoded: record.value.clone(),
            fetched_ms: now,
        })
        .unwrap();

        assert_eq!(
            cached_devices_for(&db, &account.public_key(), now),
            vec![device.public_key()]
        );
    }

    #[test]
    fn le_cache_ne_rend_rien_quand_la_liste_est_perimee() {
        // 🔒 Servir une liste périmée ferait survivre un appareil révoqué
        // aussi longtemps qu'elle. Mieux vaut ne rien rendre et forcer
        // l'appelant à rafraîchir.
        let now = 1_700_000_000_000;
        let db = db();
        let (account, _, record) = published(now);
        db.cache_device_list(&accord_core::db::CachedDeviceList {
            account: account.public_key(),
            version: version_for(now),
            encoded: record.value.clone(),
            fetched_ms: now,
        })
        .unwrap();

        let apres_expiration = now + u64::from(DEVICE_LIST_VALID_S) * 1000 + 1;
        assert!(cached_devices_for(&db, &account.public_key(), apres_expiration).is_empty());
    }

    #[test]
    fn le_cache_ne_rend_rien_pour_un_compte_inconnu() {
        let db = db();
        assert!(cached_devices_for(&db, &[3u8; 32], 1_700_000_000_000).is_empty());
    }

    /// Met en cache la liste publiée pour `account` et rend sa clé publique.
    fn en_cache(db: &Db, account: &AccountIdentity, record: &DhtRecord, now: u64) -> [u8; 32] {
        db.cache_device_list(&accord_core::db::CachedDeviceList {
            account: account.public_key(),
            version: version_for(now),
            encoded: record.value.clone(),
            fetched_ms: now,
        })
        .unwrap();
        account.public_key()
    }

    #[test]
    fn la_cle_dun_compte_ami_se_rattache_a_lui_meme() {
        // Le cas de tout le parc actuel : l'identité de transport EST
        // l'identité de compte. Il doit continuer de marcher sans liste.
        let db = db();
        let ami = Identity::generate_with_pow_bits(1).public_key();
        assert_eq!(
            account_for_static(&db, &[ami], &ami, 1_700_000_000_000),
            Some(ami)
        );
    }

    #[test]
    fn la_cle_dun_appareil_liste_se_rattache_a_son_compte() {
        let now = 1_700_000_000_000;
        let db = db();
        let (account, device, record) = published(now);
        let ami = en_cache(&db, &account, &record, now);

        assert_eq!(
            account_for_static(&db, &[ami], &device.public_key(), now),
            Some(ami)
        );
    }

    #[test]
    fn une_cle_inconnue_ne_se_rattache_a_rien() {
        let now = 1_700_000_000_000;
        let db = db();
        let (account, _, record) = published(now);
        let ami = en_cache(&db, &account, &record, now);
        let inconnu = Identity::generate_with_pow_bits(1).public_key();

        assert_eq!(account_for_static(&db, &[ami], &inconnu, now), None);
    }

    #[test]
    fn un_appareil_dont_la_liste_a_expire_ne_se_rattache_plus() {
        // 🔒 Sans cette expiration, un appareil révoqué resterait rattaché à
        // son compte tant que le cache le porte — donc indéfiniment si le
        // rafraîchissement échoue.
        let now = 1_700_000_000_000;
        let db = db();
        let (account, device, record) = published(now);
        let ami = en_cache(&db, &account, &record, now);

        let expire = now + u64::from(DEVICE_LIST_VALID_S) * 1000 + 1;
        assert_eq!(
            account_for_static(&db, &[ami], &device.public_key(), expire),
            None
        );
    }

    #[test]
    fn lappareil_dun_non_ami_ne_se_rattache_a_rien() {
        // La liste est en cache et parfaitement valide, mais son compte ne
        // figure pas parmi nos amis : elle ne doit autoriser personne.
        let now = 1_700_000_000_000;
        let db = db();
        let (account, device, record) = published(now);
        en_cache(&db, &account, &record, now);

        assert_eq!(
            account_for_static(&db, &[], &device.public_key(), now),
            None
        );
    }

    #[test]
    fn le_noeud_ne_publie_rien_tant_quaucun_appareil_nest_persiste() {
        // Une base ouverte hors du chemin de démarrage normal : pas d'appareil,
        // donc pas de liste. Mieux vaut ne rien publier qu'une liste vide, qui
        // ferait croire à un compte sans aucun appareil joignable.
        let db = db();
        assert!(db.local_device().unwrap().is_none());
    }

    #[test]
    fn la_liste_construite_depuis_la_racine_partagee_est_identique() {
        // `build_device_list_with_root` existe parce qu'`Identity` n'est pas
        // `Clone` : il doit produire exactement la même liste que la voie
        // typée, sans quoi les deux chemins divergeraient en silence.
        let root = Identity::generate_with_pow_bits(1);
        let device = DeviceIdentity::generate_with_pow_bits(1);
        let now = 1_700_000_000_000;
        let par_reference = build_device_list_with_root(&root, &device, "Portable", now, TRANSPORT);

        let account = AccountIdentity::from_identity(root);
        let par_valeur = build_device_list(&account, &device, "Portable", now, TRANSPORT);

        assert_eq!(par_reference, par_valeur);
    }

    #[test]
    fn sans_liste_connue_on_vise_la_cle_de_compte() {
        // 🔒 Le parc antérieur à 6.4 ne publie pas de liste. Rendre une liste
        // vide le rendrait injoignable du jour au lendemain.
        let db = db();
        let inconnu = [5u8; 32];
        assert_eq!(
            delivery_targets(&db, &inconnu, 1_700_000_000_000),
            vec![inconnu]
        );
    }

    #[test]
    fn avec_une_liste_fraiche_on_vise_les_appareils() {
        let now = 1_700_000_000_000;
        let db = db();
        let (account, device, record) = published(now);
        let compte = en_cache(&db, &account, &record, now);

        assert_eq!(
            delivery_targets(&db, &compte, now),
            vec![device.public_key()],
            "un message direct part vers les appareils, pas vers le compte"
        );
    }

    #[test]
    fn avec_une_liste_perimee_on_retombe_sur_le_compte() {
        // Une liste périmée ne dit plus qui est autorisé. Plutôt que de ne
        // livrer à personne, on vise le compte : le pire cas est un message
        // qui n'arrive pas, et c'est exactement ce qu'une liste vide donnerait
        // — sauf que le repli, lui, marche encore avec les pairs anciens.
        let now = 1_700_000_000_000;
        let db = db();
        let (account, _, record) = published(now);
        let compte = en_cache(&db, &account, &record, now);

        let expire = now + u64::from(DEVICE_LIST_VALID_S) * 1000 + 1;
        assert_eq!(delivery_targets(&db, &compte, expire), vec![compte]);
    }

    #[test]
    fn un_appareil_revoque_ne_recoit_plus_rien() {
        // 🔒 Le bout qui compte : après révocation, plus aucun message ne doit
        // partir vers la machine retirée.
        let now = 1_700_000_000_000;
        let db = db();
        let account = AccountIdentity::from_identity(Identity::generate_with_pow_bits(1));
        let garde = DeviceIdentity::generate_with_pow_bits(1);
        let retire = DeviceIdentity::generate_with_pow_bits(1);

        let mut liste = build_device_list(&account, &garde, "Fixe", now, TRANSPORT);
        liste.revoked.push(accord_proto::device::RevokedEntry {
            pubkey: retire.public_key(),
            revoked_ms: now,
        });
        liste.devices.push(accord_proto::device::DeviceEntry {
            pubkey: retire.public_key(),
            pow_nonce: retire.pow_nonce(),
            name: "Ancien".into(),
            added_ms: now,
            // 🔒 L'appareil retiré affirme présenter sa propre clé. C'est le
            // cas qui compte : si la révocation était vérifiée APRÈS le
            // drapeau, cette entrée se glisserait dans les cibles.
            flags: TRANSPORT,
        });
        account.sign_device_list(&mut liste);
        let record = device_list_record(&account, &liste, now);
        let compte = en_cache(&db, &account, &record, now);

        let cibles = delivery_targets(&db, &compte, now);
        assert!(cibles.contains(&garde.public_key()));
        assert!(
            !cibles.contains(&retire.public_key()),
            "un appareil révoqué ne doit plus rien recevoir"
        );
    }

    #[test]
    fn un_appareil_qui_ne_presente_pas_sa_cle_se_joint_par_le_compte() {
        // 🔒 L'état de TOUT le parc pendant la phase 1 : la liste est publiée,
        // fraîche, signée — et le transport présente encore la clé de compte.
        // Viser l'appareil ici couperait la livraison pour tout le monde le
        // jour où cette version sort.
        let now = 1_700_000_000_000;
        let db = db();
        let account = AccountIdentity::from_identity(Identity::generate_with_pow_bits(1));
        let appareil = DeviceIdentity::generate_with_pow_bits(1);
        let liste = build_device_list(&account, &appareil, "Portable", now, 0);
        let record = device_list_record(&account, &liste, now);
        let compte = en_cache(&db, &account, &record, now);

        assert_eq!(delivery_targets(&db, &compte, now), vec![compte]);
    }

    #[test]
    fn un_parc_mixte_vise_les_bascules_et_le_compte() {
        // 🔒 Le cas qui existera vraiment pendant des semaines, et celui qu'on
        // rate en simplifiant : un appareil a basculé, l'autre pas. Ne garder
        // que le premier couperait le second ; ne garder que le compte
        // couperait le premier.
        let now = 1_700_000_000_000;
        let db = db();
        let account = AccountIdentity::from_identity(Identity::generate_with_pow_bits(1));
        let bascule = DeviceIdentity::generate_with_pow_bits(1);
        let ancien = DeviceIdentity::generate_with_pow_bits(1);

        let mut liste = build_device_list(&account, &bascule, "Portable", now, TRANSPORT);
        liste.devices.push(accord_proto::device::DeviceEntry {
            pubkey: ancien.public_key(),
            pow_nonce: ancien.pow_nonce(),
            name: "Fixe".into(),
            added_ms: now,
            flags: 0,
        });
        account.sign_device_list(&mut liste);
        let record = device_list_record(&account, &liste, now);
        let compte = en_cache(&db, &account, &record, now);

        let cibles = delivery_targets(&db, &compte, now);
        assert_eq!(cibles.len(), 2, "une cible par voie distincte : {cibles:?}");
        assert!(cibles.contains(&bascule.public_key()));
        assert!(cibles.contains(&compte));
        assert!(
            !cibles.contains(&ancien.public_key()),
            "l'appareil non basculé n'écoute pas sa propre clé"
        );
    }

    #[test]
    fn deux_appareils_non_bascules_ne_font_quune_cible() {
        // Ils présentent la même clé — celle du compte — et s'évincent d'ailleurs
        // mutuellement du transport. Les compter deux fois ferait partir deux
        // exemplaires du même message vers la même session.
        let now = 1_700_000_000_000;
        let db = db();
        let account = AccountIdentity::from_identity(Identity::generate_with_pow_bits(1));
        let a = DeviceIdentity::generate_with_pow_bits(1);
        let b = DeviceIdentity::generate_with_pow_bits(1);

        let mut liste = build_device_list(&account, &a, "A", now, 0);
        liste.devices.push(accord_proto::device::DeviceEntry {
            pubkey: b.public_key(),
            pow_nonce: b.pow_nonce(),
            name: "B".into(),
            added_ms: now,
            flags: 0,
        });
        account.sign_device_list(&mut liste);
        let record = device_list_record(&account, &liste, now);
        let compte = en_cache(&db, &account, &record, now);

        assert_eq!(delivery_targets(&db, &compte, now), vec![compte]);
    }

    #[test]
    fn deux_appareils_bascules_recoivent_chacun() {
        // Le jalon en une assertion : un message direct part vers les DEUX
        // machines du destinataire.
        let now = 1_700_000_000_000;
        let db = db();
        let account = AccountIdentity::from_identity(Identity::generate_with_pow_bits(1));
        let a = DeviceIdentity::generate_with_pow_bits(1);
        let b = DeviceIdentity::generate_with_pow_bits(1);

        let mut liste = build_device_list(&account, &a, "A", now, TRANSPORT);
        liste.devices.push(accord_proto::device::DeviceEntry {
            pubkey: b.public_key(),
            pow_nonce: b.pow_nonce(),
            name: "B".into(),
            added_ms: now,
            flags: TRANSPORT,
        });
        account.sign_device_list(&mut liste);
        let record = device_list_record(&account, &liste, now);
        let compte = en_cache(&db, &account, &record, now);

        let cibles = delivery_targets(&db, &compte, now);
        assert_eq!(cibles.len(), 2);
        assert!(cibles.contains(&a.public_key()));
        assert!(cibles.contains(&b.public_key()));
        assert!(
            !cibles.contains(&compte),
            "plus personne n'écoute la clé de compte quand tout a basculé"
        );
    }

    #[test]
    fn une_liste_entierement_revoquee_retombe_sur_le_compte() {
        // Cas dégénéré : une liste fraîche dont chaque appareil est révoqué ne
        // permet pas de conclure « injoignable ». Rendre zéro cible ferait
        // disparaître le message sans trace ; la clé de compte reste le dernier
        // recours, et l'échec se constate au moins par l'absence de réponse.
        let now = 1_700_000_000_000;
        let db = db();
        let account = AccountIdentity::from_identity(Identity::generate_with_pow_bits(1));
        let seul = DeviceIdentity::generate_with_pow_bits(1);
        let mut liste = build_device_list(&account, &seul, "Portable", now, TRANSPORT);
        liste.revoked.push(accord_proto::device::RevokedEntry {
            pubkey: seul.public_key(),
            revoked_ms: now,
        });
        account.sign_device_list(&mut liste);
        let record = device_list_record(&account, &liste, now);
        let compte = en_cache(&db, &account, &record, now);

        assert_eq!(delivery_targets(&db, &compte, now), vec![compte]);
    }

    #[test]
    fn les_drapeaux_locaux_suivent_la_cle_de_transport() {
        let compte = [1u8; 32];
        let appareil = [2u8; 32];
        // Phase 1 : le transport présente la clé de COMPTE.
        assert_eq!(local_device_flags(&compte, &appareil), 0);
        // Phase 2 : il présente la clé d'appareil.
        assert_eq!(
            local_device_flags(&appareil, &appareil),
            accord_proto::device::DEVICE_FLAG_TRANSPORT_KEY
        );
    }
}
