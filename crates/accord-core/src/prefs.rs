//! Préférences de COMPTE, partagées entre les appareils (feuille de route
//! §17.4).
//!
//! Le canal filaire est [`accord_proto::core_msg::CoreMsg::SelfPref`] (0x21),
//! adressé à notre propre compte comme les autres messages `SELF_*`. Ce module
//! tient les deux choses que le protocole ne peut pas tenir : **quelles clés
//! sont acceptées**, et **qui gagne quand deux appareils changent la même**.
//!
//! ## Règle de conflit : dernier écrivain, horloge murale, par clé
//!
//! Chaque clé se résout indépendamment sur son `at_ms`, sans horloge logique.
//! `SECURITY.md` §5 item 15 note que c'est le point faible du blocage — et ça
//! l'est. Ici, ça ne l'est pas, parce que la conséquence n'est pas la même : un
//! blocage perdu est **silencieux et relève de la sécurité** (un canal qu'on
//! croyait fermé se rouvre sans que personne ne le voie), alors qu'un thème
//! perdu est **visible et se corrige tout seul** — l'utilisateur voit le
//! mauvais thème et le remet.
//!
//! Il n'y a donc aucun « côté sûr » vers lequel pencher en cas d'égalité : ni
//! le clair ni le sombre n'est le choix prudent, ni le français ni l'arabe.
//! Là où `friends::apply_remote_state` tranche les égalités en faveur du
//! blocage, on ne tranche rien du tout : à horodatage strictement égal, la
//! valeur déjà en base reste. Inventer une asymétrie ici reviendrait à
//! prétendre qu'une préférence est plus « sûre » qu'une autre.

use accord_proto::core_msg::{MAX_SELF_PREF_KEY, MAX_SELF_PREF_VALUE};
use accord_proto::{Reader, Writer};

use crate::db::Db;
use crate::error::CoreError;

/// Préférences de compte acceptées, en émission comme en réception.
///
/// 🔒 **La liste blanche est le contrôle d'admission du récepteur.** Une clé
/// absente est ignorée en silence, et c'est délibérément à double usage :
///
/// - **compatibilité ascendante** — un appareil resté sur une version
///   antérieure reçoit une préférence dont il n'a aucune notion et l'ignore,
///   au lieu de refuser le message ou de stocker un réglage qu'il ne saura
///   jamais appliquer. Ajouter une préférence n'est donc pas un changement de
///   format filaire ;
/// - **borne de dégât** — un appareil frère buggé, ou dont la graine a fuité,
///   ne peut pas remplir notre base de clés arbitraires. Le nombre de lignes
///   qu'un pair peut créer ici est fixé à la compilation, il vaut
///   `SYNCED_KEYS.len()`.
///
/// Les clés sont celles que l'interface persiste déjà dans `localStorage`
/// (`app/src/stores/ui.ts`, `STORAGE_KEYS`), à l'octet près : pas de table de
/// correspondance à tenir à jour de part et d'autre, donc pas de dérive
/// silencieuse entre les deux moitiés.
///
/// # Ce qui est REFUSÉ, et pourquoi
///
/// Le critère est : est-ce que ce réglage décrit la **personne** ou la
/// **machine** ? Seule la personne voyage.
///
/// - `accord.voice.input_device`, `accord.voice.output_device` — des noms de
///   périphérique `cpal`, exacts au caractère près. Sur une autre machine ils
///   ne sont pas seulement inutiles : ils sont **faux**, et désignent au mieux
///   rien, au pire le mauvais micro.
/// - `accord.voice.volume.*`, les trois `accord.voice.dsp.*` — calés sur un
///   micro, une pièce et une chaîne de sortie physiques. Un gain correct au
///   casque hurle sur des enceintes.
/// - `accord.fontScale`, `accord.a11y.saturation`,
///   `accord.layout.sidebarWidth`, `accord.layout.membersWidth` — affichage et
///   géométrie de fenêtre : un écran 4K de 27 pouces et un portable de 13 n'ont
///   rien à se dire là-dessus.
/// - `accord.system.keepInTray`, `accord.system.closeToTray` — la sémantique de
///   fermeture de fenêtre diffère d'un OS à l'autre ; le même booléen n'y
///   décrit pas le même comportement.
/// - `accord.network.port`, `accord.network.bootstrap`, `accord.backup.dir` —
///   réseau de la machine et chemin de fichier local. Un dossier de sauvegarde
///   valide ici n'existe pas là-bas.
/// - `accord.autoLockMinutes` — 🔒 **relève de la sécurité**. Un portable
///   partagé veut un verrouillage agressif qu'un fixe à la maison ne veut pas ;
///   synchroniser ce réglage affaiblirait précisément l'appareil qui en avait
///   le plus besoin, et le ferait en silence.
/// - `accord.notify.quietHours` — ⚠️ stocké en heures locales nues
///   (`{enabled, start: 0..23, end: 0..23}`), **sans fuseau**. Synchronisé tel
///   quel entre un appareil à Paris et un autre à Montréal, il se déclencherait
///   six heures à côté sans qu'aucune erreur ne le signale. Le faire voyager
///   demanderait d'abord de lui adjoindre un fuseau — un changement de format
///   de la valeur, pas de cette liste.
/// - `accord.profile.*` — déjà propagé par `CoreMsg::Profile` (SPEC §6.5). Le
///   doubler ici donnerait deux chemins concurrents avec deux règles de
///   conflit pour la même donnée.
/// - `accord.streamerMode` — `ui.ts` dit explicitement que c'est un réglage
///   d'**affichage** et non une garantie de confidentialité. Le faire voyager
///   d'appareil en appareil lui donnerait l'allure d'une promesse qui suit le
///   compte ; on préfère ne pas le synchroniser plutôt que d'avoir à démentir
///   cette lecture.
/// - `accord.pttEnabled`, `accord.pttKey`, `accord.privacy.startupPresence`,
///   `accord.notify.native`, `accord.channels.hideMuted`,
///   `accord.a11y.reducedMotion`, `accord.media.videoPreviewMaxMio` — clavier,
///   permission de notification système, capacités de rendu : machine.
pub const SYNCED_KEYS: &[&str] = &[
    // Langue et apparence : ce sont des goûts, pas des propriétés d'écran.
    "accord.lang",
    "accord.theme",
    "accord.theme.custom",
    "accord.density",
    "accord.timeFormat",
    "accord.appearance.fontUi",
    "accord.media.emojiSize",
    "accord.media.showPreviews",
    // Politique de notification : « quand veux-je être dérangé », qui est une
    // décision de la personne. À ne pas confondre avec `notify.native`
    // (permission accordée à CETTE machine) ni `quietHours` (sans fuseau).
    "accord.notifyDms",
    "accord.notifyGroups",
    "accord.notifyOnlyUnfocused",
    "accord.notify.soundEnabled",
    "accord.notify.soundMode",
    // Confidentialité annoncée aux pairs : la promesse porte sur la personne,
    // pas sur la machine depuis laquelle elle tape.
    "accord.privacy.typingIndicator",
];

/// Vrai si `key` est une préférence de compte connue.
#[must_use]
pub fn is_synced(key: &str) -> bool {
    SYNCED_KEYS.contains(&key)
}

/// Une préférence de compte telle qu'elle est stockée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncedPref {
    /// Clé, forcément membre de [`SYNCED_KEYS`].
    pub key: String,
    /// Valeur, telle que l'interface la persistait déjà localement.
    pub value: String,
    /// Horloge murale du dernier changement retenu, tous appareils confondus.
    pub at_ms: u64,
}

/// Clé de métadonnée d'une préférence de compte.
fn meta_key(key: &str) -> String {
    format!("pref:{key}")
}

/// Encode `(at_ms, value)` avec le **codec filaire du projet** plutôt qu'une
/// sixième disposition d'octets maison.
///
/// La table `meta` porte déjà cinq encodages ad hoc (u64 big-endian nu, octet
/// booléen, JSON, texte brut, blob opaque) ; en ajouter un septième — la paire
/// à stocker en compte deux — se paierait à la première relecture. `Writer` et
/// `Reader` sont déjà la référence de sérialisation du dépôt, déjà fuzzés, et
/// leur `lbytes` borne la longueur au décodage exactement comme sur le fil.
fn encode(value: &str, at_ms: u64) -> Vec<u8> {
    let mut w = Writer::with_capacity(8 + 4 + value.len());
    w.put_u64(at_ms);
    w.put_lbytes(value.as_bytes());
    w.into_bytes()
}

/// Décode l'inverse d'[`encode`]. Une ligne illisible (base d'une version
/// future, écriture tronquée) rend `None` : la préférence est alors traitée
/// comme absente, ce qui la laisse rattrapable par n'importe quel appareil au
/// lieu de faire échouer toute la lecture.
fn decode(raw: &[u8]) -> Option<(String, u64)> {
    let mut r = Reader::new(raw);
    let at_ms = r.u64().ok()?;
    let value = r.lbytes(MAX_SELF_PREF_VALUE, "pref.value").ok()?;
    Some((String::from_utf8(value).ok()?, at_ms))
}

/// Lit une préférence de compte (absente ⇒ `None`).
pub fn get(db: &Db, key: &str) -> Result<Option<SyncedPref>, CoreError> {
    let Some(raw) = db.meta(&meta_key(key))? else {
        return Ok(None);
    };
    Ok(decode(&raw).map(|(value, at_ms)| SyncedPref {
        key: key.to_string(),
        value,
        at_ms,
    }))
}

/// Lit toutes les préférences de compte présentes.
///
/// Parcourt [`SYNCED_KEYS`] plutôt que la table : la liste blanche est finie et
/// connue à la compilation, ce qui évite d'ajouter à `Db` un balayage par
/// préfixe dont ce serait le seul usage — et garantit au passage qu'une clé
/// retirée de la liste cesse d'être servie même si sa ligne traîne encore.
pub fn list(db: &Db) -> Result<Vec<SyncedPref>, CoreError> {
    let mut out = Vec::new();
    for key in SYNCED_KEYS {
        if let Some(pref) = get(db, key)? {
            out.push(pref);
        }
    }
    Ok(out)
}

/// Enregistre un changement décidé SUR CETTE MACHINE.
///
/// Refuse bruyamment une clé hors liste blanche ou une valeur trop longue,
/// contrairement à [`apply_remote`] qui les ignore : ici l'appelant est notre
/// propre interface, et une clé qu'elle croit synchronisée sans qu'elle le soit
/// est un bug qu'il vaut mieux voir tout de suite qu'observer six mois plus
/// tard comme « ce réglage ne suit pas ».
///
/// Écrit sans comparer les horodatages : c'est l'utilisateur qui vient de
/// cliquer, sur la machine qu'il regarde. Le refuser au motif qu'une autre
/// machine a une horloge en avance rendrait le réglage inopérant ici sans rien
/// dire.
pub fn set_local(db: &Db, key: &str, value: &str, at_ms: u64) -> Result<(), CoreError> {
    if !is_synced(key) {
        return Err(CoreError::Invalid("préférence non synchronisable"));
    }
    if value.len() > MAX_SELF_PREF_VALUE {
        return Err(CoreError::Invalid("valeur de préférence trop longue"));
    }
    db.set_meta(&meta_key(key), &encode(value, at_ms))
}

/// Applique un changement venu d'un AUTRE appareil du compte. Rend `true` si la
/// base a bougé.
///
/// 🔒 L'appelant a déjà prouvé que l'émetteur est une de nos machines (clé
/// authentifiée par la session) et que `at_ms` n'est pas dans un futur
/// invraisemblable. Il reste ici deux filtres :
///
/// - clé hors liste blanche ⇒ ignorée sans erreur (voir [`SYNCED_KEYS`]) ;
/// - horodatage antérieur ou **égal** à celui en base ⇒ ignoré. Strictement
///   supérieur pour gagner : à égalité on ne sait pas départager, et il n'y a
///   pas de valeur « prudente » vers laquelle pencher (voir l'en-tête du
///   module). Garder l'existant rend au moins l'opération idempotente — le même
///   message rejoué deux fois ne fait rien la seconde fois.
pub fn apply_remote(db: &Db, key: &str, value: &str, at_ms: u64) -> Result<bool, CoreError> {
    if !is_synced(key) || value.len() > MAX_SELF_PREF_VALUE || key.len() > MAX_SELF_PREF_KEY {
        return Ok(false);
    }
    if let Some(courant) = get(db, key)? {
        if at_ms <= courant.at_ms {
            return Ok(false);
        }
    }
    db.set_meta(&meta_key(key), &encode(value, at_ms))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        Db::open_in_memory(&[7u8; 32]).expect("base mémoire")
    }

    #[test]
    fn set_local_then_get_roundtrips() {
        let db = db();
        assert_eq!(get(&db, "accord.theme").unwrap(), None);
        set_local(&db, "accord.theme", "midnight", 1_000).unwrap();
        assert_eq!(
            get(&db, "accord.theme").unwrap(),
            Some(SyncedPref {
                key: "accord.theme".into(),
                value: "midnight".into(),
                at_ms: 1_000,
            })
        );
    }

    #[test]
    fn set_local_refuses_a_key_outside_the_allowlist() {
        let db = db();
        // Un réglage de machine, refusé bruyamment côté émission.
        assert!(set_local(&db, "accord.autoLockMinutes", "5", 1).is_err());
        assert!(set_local(&db, "accord.voice.input_device", "MacBook", 1).is_err());
    }

    #[test]
    fn an_unknown_key_from_a_sibling_is_ignored_without_error() {
        let db = db();
        // 🔒 Ignoré, pas rejeté : c'est ce qui rend le message compatible avec
        // une version antérieure ET ce qui empêche un frère buggé de remplir
        // la base de clés arbitraires.
        assert!(!apply_remote(&db, "accord.inventee", "x", 9_000).unwrap());
        assert!(!apply_remote(&db, "accord.autoLockMinutes", "1", 9_000).unwrap());
        assert_eq!(get(&db, "accord.autoLockMinutes").unwrap(), None);
        assert!(list(&db).unwrap().is_empty());
    }

    #[test]
    fn last_writer_wins_in_both_directions() {
        let db = db();
        set_local(&db, "accord.lang", "fr", 1_000).unwrap();

        // Plus récent : gagne.
        assert!(apply_remote(&db, "accord.lang", "de", 2_000).unwrap());
        assert_eq!(get(&db, "accord.lang").unwrap().unwrap().value, "de");

        // Plus ancien : perd, et la base ne bouge pas.
        assert!(!apply_remote(&db, "accord.lang", "es", 1_500).unwrap());
        assert_eq!(get(&db, "accord.lang").unwrap().unwrap().value, "de");

        // À égalité stricte : rien ne bouge non plus (pas de tie-break).
        assert!(!apply_remote(&db, "accord.lang", "pt", 2_000).unwrap());
        assert_eq!(get(&db, "accord.lang").unwrap().unwrap().value, "de");

        // Une décision locale postérieure reprend la main.
        set_local(&db, "accord.lang", "ar", 3_000).unwrap();
        assert_eq!(get(&db, "accord.lang").unwrap().unwrap().value, "ar");
    }

    #[test]
    fn an_oversized_value_is_refused_locally_and_ignored_remotely() {
        let db = db();
        let trop = "x".repeat(MAX_SELF_PREF_VALUE + 1);
        assert!(set_local(&db, "accord.theme.custom", &trop, 1).is_err());
        assert!(!apply_remote(&db, "accord.theme.custom", &trop, 1).unwrap());
        assert_eq!(get(&db, "accord.theme.custom").unwrap(), None);
    }

    #[test]
    fn list_only_returns_allowlisted_keys_that_are_present() {
        let db = db();
        set_local(&db, "accord.lang", "fr", 1).unwrap();
        set_local(&db, "accord.density", "compact", 2).unwrap();
        let keys: Vec<String> = list(&db).unwrap().into_iter().map(|p| p.key).collect();
        assert_eq!(keys, vec!["accord.lang", "accord.density"]);
    }

    #[test]
    fn a_corrupt_row_reads_as_absent_rather_than_failing() {
        let db = db();
        db.set_meta(&meta_key("accord.theme"), b"\x00\x01").unwrap();
        assert_eq!(get(&db, "accord.theme").unwrap(), None);
        // Et reste rattrapable : rien ne verrouille la clé.
        assert!(apply_remote(&db, "accord.theme", "ocean", 5).unwrap());
    }

    #[test]
    fn a_non_utf8_row_also_reads_as_absent() {
        // Ne peut pas venir de nos deux chemins d'écriture (tous deux prennent
        // une `&str`) ; couvert quand même, parce que « impossible » et
        // « impossible à faire paniquer » ne sont pas la même propriété.
        let db = db();
        let mut w = Writer::new();
        w.put_u64(42);
        w.put_lbytes(&[0xFF, 0xFE]);
        db.set_meta(&meta_key("accord.lang"), &w.into_bytes())
            .unwrap();
        assert_eq!(get(&db, "accord.lang").unwrap(), None);
    }

    #[test]
    fn the_allowlist_holds_no_duplicate_and_no_oversized_key() {
        let mut vus = std::collections::HashSet::new();
        for key in SYNCED_KEYS {
            assert!(vus.insert(*key), "clé en double dans la liste blanche");
            assert!(key.len() <= MAX_SELF_PREF_KEY, "clé au-delà de la borne");
        }
    }
}
