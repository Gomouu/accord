//! Stockage clé-valeur signé de la DHT : validation, expiration, quotas et
//! republication (SPEC §4).

use accord_crypto::{device_list_key, verify_signature, FriendCode, FRIENDCODE_PAYLOAD_LEN};
use accord_proto::limits::{DHT_MAX_EXPIRY_S, MAX_DHT_VALUE};
use accord_proto::types::{DhtRecord, RecordKind};
use std::collections::HashMap;

/// Longueur exacte de la valeur d'un record IDENTITY : payload de code ami
/// (8 octets, SPEC §5) suivi de la clé publique Ed25519 du publieur (32 octets).
const IDENTITY_VALUE_LEN: usize = FRIENDCODE_PAYLOAD_LEN + 32;

/// Dérive d'horloge tolérée sur l'horodatage d'un record (5 minutes). Au-delà,
/// un record « du futur » est rejeté : sans cette borne, un attaquant fixant
/// `timestamp_ms = u64::MAX` épinglerait sa valeur de façon permanente (jamais
/// remplaçable, car `put` conserve toujours l'horodatage le plus élevé).
pub const MAX_CLOCK_SKEW_MS: u64 = 5 * 60 * 1000;

/// Un seul publieur ne peut occuper qu'au plus `1/PUBLISHER_SHARE_DIVISOR` des
/// emplacements du magasin : borne anti-épuisement empêchant qu'une identité
/// unique remplisse le stockage (l'éviction globale ne suffit pas à l'en
/// empêcher).
const PUBLISHER_SHARE_DIVISOR: usize = 16;

/// Plancher du quota par publieur, pour les magasins de petite taille (tests,
/// réseaux réduits) : on autorise toujours au moins ce nombre de records par
/// publieur, quelle que soit la capacité globale.
const MIN_RECORDS_PER_PUBLISHER: usize = 8;

/// Erreur de validation d'un record à stocker.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StoreError {
    /// Signature du publieur invalide.
    #[error("signature invalide")]
    BadSignature,
    /// La clé ne correspond pas au contenu pour ce kind.
    #[error("clé incohérente avec le contenu")]
    KeyMismatch,
    /// Valeur trop grande.
    #[error("valeur trop grande")]
    TooLarge,
    /// Expiration hors bornes.
    #[error("expiration invalide")]
    BadExpiry,
    /// Genre de record introduit par une version plus récente : le décodage
    /// l'a laissé passer (pour ne pas condamner toute la réponse), mais ce
    /// nœud ne sait pas le valider et refuse donc de l'héberger.
    #[error("genre de record inconnu")]
    UnknownKind,
    /// Horodatage trop loin dans le futur (dérive d'horloge dépassée).
    #[error("horodatage hors bornes")]
    FutureTimestamp,
    /// Quota de records atteint pour ce publieur.
    #[error("quota par publieur atteint")]
    PublisherQuota,
    /// Espace de stockage plein.
    #[error("stockage plein")]
    Full,
}

struct StoredRecord {
    record: DhtRecord,
    expires_at_ms: u64,
}

/// Magasin local de records DHT, borné en nombre d'entrées et par publieur.
pub struct RecordStore {
    records: HashMap<[u8; 32], StoredRecord>,
    max_records: usize,
    max_per_publisher: usize,
}

/// Vérifie qu'un record IDENTITY est intégralement lié à son publieur (SPEC §5).
///
/// Un record IDENTITY publie « code ami → identité complète ». Le payload de
/// 8 octets étant public (dérivé du code ami affiché), ne vérifier que
/// `clé == SHA-256("friendcode-v1" ‖ payload)` laisserait n'importe qui forger
/// un record auto-signé sous la clé DHT d'une victime et squatter son
/// emplacement. On exige donc toute la chaîne `clé ← payload ← publieur` :
/// - la valeur fait exactement `FRIENDCODE_PAYLOAD_LEN + 32` octets
///   (`payload ‖ clé publique`) ;
/// - les 32 octets de clé publique portés par la valeur égalent
///   `record.publisher` ;
/// - le payload de 8 octets est bien celui dérivé de `record.publisher`
///   (`FriendCode::of_pubkey`, soit `SHA-256(pubkey)[..8]`) ;
/// - la clé DHT est le hash ancré de ce payload (`FriendCode::dht_key`).
///
/// Un payload public ne peut alors plus être « re-signé » par un autre
/// publieur : sa clé DHT ne correspondrait pas à celle de la victime.
fn identity_record_is_bound(record: &DhtRecord) -> bool {
    if record.value.len() != IDENTITY_VALUE_LEN {
        return false;
    }
    // La clé publique portée par la valeur doit être celle du publieur.
    if record.value[FRIENDCODE_PAYLOAD_LEN..] != record.publisher {
        return false;
    }
    // Le payload doit dériver du publieur, et la clé DHT ancrer ce payload.
    let code = FriendCode::of_pubkey(&record.publisher);
    record.value[..FRIENDCODE_PAYLOAD_LEN] == *code.payload() && record.key == code.dht_key()
}

impl RecordStore {
    /// Crée un magasin bornant le nombre de records conservés (globalement et
    /// par publieur).
    pub fn new(max_records: usize) -> Self {
        let max_per_publisher =
            (max_records / PUBLISHER_SHARE_DIVISOR).max(MIN_RECORDS_PER_PUBLISHER);
        Self {
            records: HashMap::new(),
            max_records,
            max_per_publisher,
        }
    }

    /// Valide un record indépendamment du stockage (utilisable côté lecteur
    /// pour rejeter une valeur DHT non authentique).
    pub fn validate(record: &DhtRecord) -> Result<(), StoreError> {
        if record.value.len() > MAX_DHT_VALUE {
            return Err(StoreError::TooLarge);
        }
        if record.expiry_s == 0 || record.expiry_s > DHT_MAX_EXPIRY_S {
            return Err(StoreError::BadExpiry);
        }
        verify_signature(&record.publisher, &record.signable_bytes(), &record.sig)
            .map_err(|_| StoreError::BadSignature)?;
        match record.kind {
            RecordKind::Identity => {
                if !identity_record_is_bound(record) {
                    return Err(StoreError::KeyMismatch);
                }
            }
            RecordKind::DeviceList => {
                // La clé doit dériver du publieur, comme pour IDENTITY : sans
                // cet ancrage, n'importe qui pourrait publier SA liste
                // d'appareils à la clé d'un autre compte et occuper la place —
                // la signature seule n'empêche pas de viser la mauvaise porte.
                if record.key != device_list_key(&record.publisher) {
                    return Err(StoreError::KeyMismatch);
                }
            }
            RecordKind::Presence | RecordKind::MailboxHint | RecordKind::FileProvider => {
                // La clé est libre (dérivée applicative) mais le record reste
                // signé par son publieur, ce qui suffit à l'authentifier.
            }
            // Stocker ce qu'on ne sait pas valider reviendrait à offrir un
            // espace de stockage arbitraire, signé par n'importe qui.
            RecordKind::Unknown(_) => return Err(StoreError::UnknownKind),
        }
        Ok(())
    }

    /// Stocke un record après validation. Conserve la version la plus récente
    /// en cas de collision de clé (par horodatage). Applique la borne
    /// d'horodatage (anti-épinglage) puis le quota par publieur (anti-épuisement).
    pub fn put(&mut self, record: DhtRecord, now_ms: u64) -> Result<(), StoreError> {
        Self::validate(&record)?;
        // Rejette un record daté trop loin dans le futur : autrement, un
        // `timestamp_ms` gonflé le rendrait définitivement irremplaçable.
        if record.timestamp_ms > now_ms.saturating_add(MAX_CLOCK_SKEW_MS) {
            return Err(StoreError::FutureTimestamp);
        }
        let expires_at_ms = now_ms + record.expiry_s as u64 * 1000;

        if let Some(existing) = self.records.get(&record.key) {
            if existing.record.timestamp_ms >= record.timestamp_ms {
                return Ok(()); // on garde la version au moins aussi récente
            }
            // Remplacement d'une clé existante : la taille du magasin ne change
            // pas, on ne touche donc ni au quota par publieur ni à l'éviction.
        } else {
            // Nouvelle clé : quota par publieur d'abord (un publieur ne peut pas
            // saturer le magasin à lui seul), puis capacité globale.
            if self.publisher_record_count(&record.publisher) >= self.max_per_publisher {
                return Err(StoreError::PublisherQuota);
            }
            if self.records.len() >= self.max_records {
                // Éviction du record le plus proche de l'expiration (LRU par TTL).
                if let Some(victim) = self
                    .records
                    .iter()
                    .min_by_key(|(_, r)| r.expires_at_ms)
                    .map(|(k, _)| *k)
                {
                    if self.records[&victim].expires_at_ms <= expires_at_ms {
                        self.records.remove(&victim);
                    } else {
                        return Err(StoreError::Full);
                    }
                }
            }
        }
        self.records.insert(
            record.key,
            StoredRecord {
                record,
                expires_at_ms,
            },
        );
        Ok(())
    }

    /// Nombre de records actuellement détenus pour un publieur donné (base du
    /// quota anti-épuisement).
    fn publisher_record_count(&self, publisher: &[u8; 32]) -> usize {
        self.records
            .values()
            .filter(|r| &r.record.publisher == publisher)
            .count()
    }

    /// Récupère un record valide (non expiré) par clé.
    pub fn get(&self, key: &[u8; 32], now_ms: u64) -> Option<DhtRecord> {
        self.records
            .get(key)
            .filter(|r| r.expires_at_ms > now_ms)
            .map(|r| r.record.clone())
    }

    /// Supprime les records expirés ; rend le nombre supprimé.
    pub fn expire(&mut self, now_ms: u64) -> usize {
        let before = self.records.len();
        self.records.retain(|_, r| r.expires_at_ms > now_ms);
        before - self.records.len()
    }

    /// Records republiés par ce nœud (tous ceux qu'il détient encore valides).
    pub fn all_valid(&self, now_ms: u64) -> Vec<DhtRecord> {
        self.records
            .values()
            .filter(|r| r.expires_at_ms > now_ms)
            .map(|r| r.record.clone())
            .collect()
    }

    /// Nombre de records stockés.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Vrai si le magasin est vide.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accord_crypto::Identity;

    fn signed_record(id: &Identity, kind: RecordKind, key: [u8; 32], value: Vec<u8>) -> DhtRecord {
        let mut rec = DhtRecord {
            key,
            kind,
            value,
            publisher: id.public_key(),
            timestamp_ms: 1000,
            expiry_s: 3600,
            sig: [0; 64],
        };
        rec.sig = id.sign(&rec.signable_bytes());
        rec
    }

    /// Record IDENTITY intégralement lié à `id` : payload dérivé de sa clé,
    /// valeur `payload ‖ clé publique`, clé DHT ancrée sur le payload.
    fn identity_record(id: &Identity) -> DhtRecord {
        let code = FriendCode::of_pubkey(&id.public_key());
        let key = code.dht_key();
        let mut value = code.payload().to_vec();
        value.extend_from_slice(&id.public_key());
        signed_record(id, RecordKind::Identity, key, value)
    }

    /// Record PRESENCE (clé applicative libre) : seul le publieur signe. Le
    /// `tag` rend les clés distinctes pour un même publieur (test de quota).
    fn presence_record(id: &Identity, tag: u8) -> DhtRecord {
        signed_record(id, RecordKind::Presence, [tag; 32], vec![tag])
    }

    /// Record DEVICE_LIST publié à sa propre clé, comme il se doit.
    fn device_list_record(id: &Identity) -> DhtRecord {
        let key = device_list_key(&id.public_key());
        signed_record(id, RecordKind::DeviceList, key, vec![7; 16])
    }

    #[test]
    fn une_liste_dappareils_publiee_a_sa_propre_cle_est_acceptee() {
        let id = Identity::generate_with_pow_bits(1);
        assert!(RecordStore::validate(&device_list_record(&id)).is_ok());
    }

    #[test]
    fn une_liste_dappareils_publiee_a_la_cle_dun_autre_compte_est_refusee() {
        // Sans cet ancrage, Mallory occuperait la place d'Alice dans la DHT :
        // sa liste est parfaitement signée — par elle-même — et les pairs qui
        // cherchent celle d'Alice ne trouveraient plus que celle-là.
        let alice = Identity::generate_with_pow_bits(1);
        let mallory = Identity::generate_with_pow_bits(1);
        let mut squat = device_list_record(&mallory);
        squat.key = device_list_key(&alice.public_key());
        squat.sig = mallory.sign(&squat.signable_bytes());

        assert!(matches!(
            RecordStore::validate(&squat),
            Err(StoreError::KeyMismatch)
        ));
    }

    #[test]
    fn un_genre_inconnu_reste_refuse_au_stockage() {
        // La tolérance au décodage (6.3) ne doit pas devenir une tolérance au
        // stockage : accueillir ce qu'on ne sait pas valider offrirait un
        // espace de stockage arbitraire à tout publieur signé.
        let id = Identity::generate_with_pow_bits(1);
        let rec = signed_record(&id, RecordKind::Unknown(0x42), [1; 32], vec![0]);
        assert!(matches!(
            RecordStore::validate(&rec),
            Err(StoreError::UnknownKind)
        ));
    }

    #[test]
    fn put_get_roundtrip() {
        let id = Identity::generate_with_pow_bits(1);
        let mut store = RecordStore::new(100);
        let rec = identity_record(&id);
        let key = rec.key;
        store.put(rec, 0).unwrap();
        assert!(store.get(&key, 0).is_some());
    }

    #[test]
    fn tampered_signature_rejected() {
        let id = Identity::generate_with_pow_bits(1);
        let mut rec = identity_record(&id);
        rec.value[6] ^= 1; // altère après signature
        let mut store = RecordStore::new(100);
        assert_eq!(store.put(rec, 0), Err(StoreError::BadSignature));
    }

    #[test]
    fn identity_key_mismatch_rejected() {
        let id = Identity::generate_with_pow_bits(1);
        let mut rec = identity_record(&id);
        rec.key[0] ^= 0xFF;
        rec.sig = id.sign(&rec.signable_bytes()); // re-signe la clé fausse
        let mut store = RecordStore::new(100);
        assert_eq!(store.put(rec, 0), Err(StoreError::KeyMismatch));
    }

    #[test]
    fn expiry_bounds_enforced() {
        let id = Identity::generate_with_pow_bits(1);
        let mut rec = identity_record(&id);
        rec.expiry_s = DHT_MAX_EXPIRY_S + 1;
        rec.sig = id.sign(&rec.signable_bytes());
        assert_eq!(RecordStore::validate(&rec), Err(StoreError::BadExpiry));
    }

    #[test]
    fn newer_timestamp_wins() {
        let id = Identity::generate_with_pow_bits(1);
        let mut store = RecordStore::new(100);
        let mut rec = identity_record(&id);
        let key = rec.key;
        store.put(rec.clone(), 0).unwrap();
        rec.timestamp_ms = 2000; // même clé/publieur, version plus récente
        rec.sig = id.sign(&rec.signable_bytes());
        store.put(rec, 0).unwrap();
        assert_eq!(store.get(&key, 0).unwrap().timestamp_ms, 2000);
    }

    #[test]
    fn identity_publisher_mismatch_rejected() {
        // Faille #1 : un attaquant reprend le payload public d'une victime et
        // signe le record avec SA propre clé. La clé DHT (ancrée sur le payload
        // de la victime) ne correspond plus à son publieur : rejet au stockage.
        let victime = Identity::generate_with_pow_bits(1);
        let attaquant = Identity::generate_with_pow_bits(1);
        let victime_code = FriendCode::of_pubkey(&victime.public_key());

        let mut value = victime_code.payload().to_vec();
        value.extend_from_slice(&attaquant.public_key());
        let forge = signed_record(
            &attaquant,
            RecordKind::Identity,
            victime_code.dht_key(), // emplacement DHT de la victime
            value,
        );
        assert_eq!(RecordStore::validate(&forge), Err(StoreError::KeyMismatch));
    }

    #[test]
    fn identity_wrong_value_length_rejected() {
        // Faille #1 : une valeur qui n'est pas exactement `payload ‖ pubkey`
        // (ici sans la clé publique) est rejetée.
        let id = Identity::generate_with_pow_bits(1);
        let code = FriendCode::of_pubkey(&id.public_key());
        let forge = signed_record(
            &id,
            RecordKind::Identity,
            code.dht_key(),
            code.payload().to_vec(), // 8 octets, clé publique absente
        );
        assert_eq!(RecordStore::validate(&forge), Err(StoreError::KeyMismatch));
    }

    #[test]
    fn future_timestamp_rejected() {
        // Faille #2 : un record daté très loin dans le futur est refusé, ce qui
        // empêche l'épinglage permanent (record jamais remplaçable).
        let id = Identity::generate_with_pow_bits(1);
        let mut store = RecordStore::new(100);
        let mut rec = identity_record(&id);
        rec.timestamp_ms = u64::MAX;
        rec.sig = id.sign(&rec.signable_bytes());
        assert_eq!(store.put(rec, 1000), Err(StoreError::FutureTimestamp));
    }

    #[test]
    fn timestamp_within_skew_accepted() {
        // Faille #2 : un léger décalage d'horloge (dans la tolérance) reste admis.
        let id = Identity::generate_with_pow_bits(1);
        let mut store = RecordStore::new(100);
        let mut rec = identity_record(&id);
        rec.timestamp_ms = 1000 + MAX_CLOCK_SKEW_MS;
        rec.sig = id.sign(&rec.signable_bytes());
        assert!(store.put(rec, 1000).is_ok());
    }

    #[test]
    fn publisher_quota_enforced() {
        // Faille #4 : un publieur ne peut pas saturer le magasin à lui seul.
        let id = Identity::generate_with_pow_bits(1);
        let mut store = RecordStore::new(100);
        let quota = (100 / PUBLISHER_SHARE_DIVISOR).max(MIN_RECORDS_PER_PUBLISHER);

        for tag in 0..quota as u8 {
            store.put(presence_record(&id, tag), 0).unwrap();
        }
        // Le record excédentaire (clé distincte, même publieur) est refusé.
        assert_eq!(
            store.put(presence_record(&id, 200), 0),
            Err(StoreError::PublisherQuota)
        );
        // Un autre publieur reste admis : le quota est bien par publieur.
        let autre = Identity::generate_with_pow_bits(1);
        assert!(store.put(presence_record(&autre, 250), 0).is_ok());
    }

    #[test]
    fn expiration_removes_records() {
        let id = Identity::generate_with_pow_bits(1);
        let mut store = RecordStore::new(100);
        let rec = identity_record(&id); // expiry 3600 s
        let key = rec.key;
        store.put(rec, 0).unwrap();
        assert!(store.get(&key, 3_600_000 + 1).is_none());
        assert_eq!(store.expire(3_600_000 + 1), 1);
        assert!(store.is_empty());
    }
    #[test]
    fn record_de_genre_inconnu_refuse_au_stockage() {
        // Le décodage le laisse passer (voir `unknown_record_kind_survives_decoding`),
        // mais l'héberger reviendrait à offrir un espace de stockage arbitraire
        // à n'importe quel publieur signé.
        let id = Identity::generate_with_pow_bits(1);
        let record = signed_record(&id, RecordKind::Unknown(0x7f), [4; 32], vec![1, 2, 3]);
        assert_eq!(RecordStore::validate(&record), Err(StoreError::UnknownKind));

        let mut store = RecordStore::new(100);
        assert_eq!(store.put(record, 0), Err(StoreError::UnknownKind));
    }
}
