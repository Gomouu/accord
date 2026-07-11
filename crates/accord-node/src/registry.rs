//! Registre local des comptes (multi-profils, D-046).
//!
//! Un compte Accord n'existe que dans ses fichiers locaux chiffrés (coffre
//! `identity.vault` + base `accord.db`) — ce module n'ajoute qu'un fichier
//! **en clair** (`profiles.json`) listant leurs métadonnées d'affichage
//! (nom, horodatages), pour que l'hôte puisse peupler un sélecteur de
//! comptes *avant* tout déverrouillage. Aucun secret n'y transite jamais.
//!
//! Robustesse : le registre n'est qu'un **cache** dérivable du système de
//! fichiers. S'il est absent ou corrompu, il est reconstruit par balayage du
//! répertoire (tout sous-répertoire contenant `identity.vault` redevient un
//! compte) — jamais par suppression. Aucune fonction de ce module ne
//! supprime de fichier ou de répertoire ; il n'existe, volontairement,
//! aucune opération de suppression de compte dans cette vague (voir
//! DECISIONS.md, D-046).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use crate::error::NodeError;
use crate::hex;
use crate::identity::Paths;
use crate::node::now_ms;

/// Nom du fichier de registre, à la racine du répertoire de données de
/// l'application (frère du répertoire de profil historique).
pub const REGISTRY_FILE: &str = "profiles.json";
/// Nom du répertoire de profil historique (un seul compte, D-045 et
/// antérieures) : jamais déplacé ni renommé par la migration multi-comptes.
pub const LEGACY_DIR_NAME: &str = "profil";
/// Sous-répertoire racine des profils créés depuis l'introduction du
/// multi-compte : chacun vit dans `profiles/<id>/`.
pub const PROFILES_DIR_NAME: &str = "profiles";
/// Identifiant réservé, stable, du profil historique une fois migré dans le
/// registre.
pub const LEGACY_ID: &str = "default";
/// Nom affiché par défaut du profil historique tant qu'aucun pseudo n'a pu
/// être lu depuis son profil applicatif (rafraîchi après le premier
/// déverrouillage réussi — voir `EtatHote::rafraichir_compte_actif` côté
/// hôte Tauri).
pub const LEGACY_PLACEHOLDER_NAME: &str = "Mon compte";

/// Version du schéma du fichier de registre (migrations futures).
const SCHEMA_VERSION: u32 = 1;

/// Préfixe du nom provisoire attribué à un compte retrouvé par balayage
/// (registre absent ou corrompu) : son vrai pseudo, s'il en a un, sera
/// rétabli au prochain déverrouillage réussi.
const DISCOVERED_NAME_PREFIX: &str = "Compte ";

/// Une entrée du registre : métadonnées d'affichage d'un compte local,
/// jamais de secret. `dir_name` est relatif à la racine du registre
/// (répertoire de données de l'application).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountEntry {
    /// Identifiant stable du compte (`"default"` pour le profil historique,
    /// sinon un identifiant aléatoire opaque).
    pub id: String,
    /// Sous-répertoire du profil, relatif à la racine du registre.
    pub dir_name: String,
    /// Nom affiché dans le sélecteur de comptes (métadonnée en clair,
    /// compromis assumé — voir DECISIONS.md D-046).
    pub name: String,
    /// Date de création estimée (millisecondes Unix ; best-effort pour un
    /// compte retrouvé par balayage plutôt que créé par ce registre).
    pub created_ms: u64,
    /// Date de dernière utilisation (millisecondes Unix), pour trier le
    /// sélecteur et choisir le profil actif par défaut au démarrage.
    pub last_used_ms: u64,
    /// Clé publique Ed25519 en hexadécimal, si connue (rafraîchie après un
    /// déverrouillage) : aide à distinguer deux comptes de même nom. Jamais
    /// requise, jamais un secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pubkey_hex: Option<String>,
}

/// Forme sur disque du fichier de registre.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistryFile {
    /// Version du schéma (migrations futures).
    #[serde(default)]
    schema_version: u32,
    /// Comptes connus.
    #[serde(default)]
    accounts: Vec<AccountEntry>,
}

/// Registre des comptes locaux, raciné sur le répertoire de données de
/// l'application. Sans état interne mis en cache : chaque appel relit (et,
/// si besoin, réécrit) `profiles.json`, pour rester correct même si le
/// fichier est modifié entre deux appels (installation unique par poste,
/// coût d'E/S négligeable pour un fichier de quelques comptes).
pub struct Registry {
    root: PathBuf,
    /// Sérialise les séquences lecture-modification-écriture : deux
    /// commandes Tauri concurrentes (ex. deux bascules de compte quasi
    /// simultanées) ne doivent jamais s'écraser l'une l'autre.
    write_lock: Mutex<()>,
}

impl Registry {
    /// Construit le registre pour un répertoire de données d'application.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            write_lock: Mutex::new(()),
        }
    }

    /// Chemins du profil historique (jamais déplacé).
    pub fn legacy_paths(&self) -> Paths {
        Paths::new(self.root.join(LEGACY_DIR_NAME))
    }

    /// Chemins du profil d'une entrée du registre.
    pub fn paths_of(&self, entry: &AccountEntry) -> Paths {
        Paths::new(self.root.join(&entry.dir_name))
    }

    /// Charge la liste des comptes connus.
    ///
    /// Tolérant par construction :
    /// - fichier absent ou illisible/corrompu → reconstruction par balayage
    ///   du répertoire (tout sous-répertoire, y compris historique, dont
    ///   `identity.vault` existe redevient un compte) ;
    /// - profil historique présent sur disque mais pas encore référencé →
    ///   migré (une seule fois ; idempotent aux appels suivants).
    ///
    /// N'écrit sur disque que si le contenu chargé diffère de ce qui serait
    /// relu tel quel (fichier absent, corrompu, ou migration légale) —
    /// jamais lors d'une lecture déjà à jour.
    pub fn load_or_init(&self) -> Result<Vec<AccountEntry>, NodeError> {
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.load_or_init_locked()
    }

    /// Compte correspondant à `id`, s'il existe.
    pub fn get(&self, id: &str) -> Result<Option<AccountEntry>, NodeError> {
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Ok(self.load_or_init_locked()?.into_iter().find(|a| a.id == id))
    }

    /// Tous les comptes, triés du plus récemment utilisé au moins récent.
    pub fn list(&self) -> Result<Vec<AccountEntry>, NodeError> {
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut accounts = self.load_or_init_locked()?;
        accounts.sort_by_key(|a| std::cmp::Reverse(a.last_used_ms));
        Ok(accounts)
    }

    /// Compte le plus récemment utilisé, s'il en existe au moins un.
    pub fn most_recently_used(&self) -> Result<Option<AccountEntry>, NodeError> {
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Ok(self
            .load_or_init_locked()?
            .into_iter()
            .max_by_key(|a| a.last_used_ms))
    }

    /// Alloue un nouvel identifiant et les chemins de son répertoire de
    /// profil (`profiles/<id>/`), **sans encore rien persister** : à
    /// appeler avant de tenter de sceller une identité neuve sur ces
    /// chemins. N'appeler [`Registry::register`] qu'après le succès du
    /// scellement, pour ne jamais référencer un profil vide ou à moitié
    /// créé dans le registre.
    pub fn new_entry(&self, name: impl Into<String>) -> (AccountEntry, Paths) {
        let id = generate_id();
        let dir_name = format!("{PROFILES_DIR_NAME}/{id}");
        let paths = Paths::new(self.root.join(&dir_name));
        let now = now_ms();
        let entry = AccountEntry {
            id,
            dir_name,
            name: name.into(),
            created_ms: now,
            last_used_ms: now,
            pubkey_hex: None,
        };
        (entry, paths)
    }

    /// Enregistre un compte fraîchement créé (après succès du scellement de
    /// son identité). Upsert par sécurité (remplace toute entrée de même
    /// identifiant), mais l'identifiant aléatoire de [`Registry::new_entry`]
    /// rend une collision pratiquement impossible.
    pub fn register(&self, entry: AccountEntry) -> Result<(), NodeError> {
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut accounts = self.load_or_init_locked()?;
        accounts.retain(|a| a.id != entry.id);
        accounts.push(entry);
        self.write_locked(&accounts)
    }

    /// Marque un compte comme utilisé à l'instant (`last_used_ms`) et
    /// rafraîchit son nom affiché / sa clé publique si fournis (jamais
    /// effacés par une valeur absente ou vide). Si l'identifiant est
    /// inconnu du registre :
    /// - le profil historique (`LEGACY_ID`) est inséré à la volée (couvre le
    ///   tout premier `create_identity`/`unlock` d'une installation, avant
    ///   toute migration explicite) ;
    /// - tout autre identifiant est ignoré (avec un avertissement journalisé)
    ///   plutôt que de fabriquer une correspondance de répertoire arbitraire.
    pub fn record_use(
        &self,
        id: &str,
        name: Option<String>,
        pubkey_hex: Option<String>,
    ) -> Result<(), NodeError> {
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut accounts = self.load_or_init_locked()?;
        let now = now_ms();
        let clean_name = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        match accounts.iter_mut().find(|a| a.id == id) {
            Some(entry) => {
                entry.last_used_ms = now;
                if let Some(n) = clean_name {
                    entry.name = n;
                }
                if let Some(pk) = pubkey_hex {
                    entry.pubkey_hex = Some(pk);
                }
            }
            None if id == LEGACY_ID => {
                accounts.push(AccountEntry {
                    id: LEGACY_ID.to_string(),
                    dir_name: LEGACY_DIR_NAME.to_string(),
                    name: clean_name.unwrap_or_else(|| LEGACY_PLACEHOLDER_NAME.to_string()),
                    created_ms: now,
                    last_used_ms: now,
                    pubkey_hex,
                });
            }
            None => {
                tracing::warn!(
                    id,
                    "registre de comptes : identifiant inconnu, mise à jour ignorée"
                );
                return Ok(());
            }
        }
        self.write_locked(&accounts)
    }

    /// Charge (avec tolérance + migration) sans reprendre le verrou —
    /// réservé aux méthodes publiques qui le tiennent déjà.
    fn load_or_init_locked(&self) -> Result<Vec<AccountEntry>, NodeError> {
        let (mut accounts, mut changed) = match self.read_raw() {
            Some(file) => (file.accounts, false),
            None => (self.scan(), true),
        };
        if self.legacy_paths().has_identity()
            && !accounts.iter().any(|a| a.dir_name == LEGACY_DIR_NAME)
        {
            accounts.push(self.legacy_entry());
            changed = true;
        }
        if changed {
            self.write_locked(&accounts)?;
        }
        Ok(accounts)
    }

    /// Lit et parse `profiles.json` ; `None` si absent ou illisible/corrompu
    /// (jamais une erreur dure — c'est le déclencheur de la reconstruction
    /// par balayage).
    fn read_raw(&self) -> Option<RegistryFile> {
        let bytes = std::fs::read(self.registry_path()).ok()?;
        serde_json::from_slice::<RegistryFile>(&bytes).ok()
    }

    /// Reconstruit la liste des comptes en balayant le répertoire : le
    /// profil historique s'il a un coffre, puis tout `profiles/<id>/` avec
    /// coffre. Utilisé quand `profiles.json` est absent ou corrompu — aucune
    /// suppression n'a lieu, seule une redécouverte des profils existants.
    fn scan(&self) -> Vec<AccountEntry> {
        let mut out = Vec::new();
        if self.legacy_paths().has_identity() {
            out.push(self.legacy_entry());
        }
        let profiles_root = self.root.join(PROFILES_DIR_NAME);
        let Ok(read) = std::fs::read_dir(&profiles_root) else {
            return out;
        };
        let mut entries: Vec<_> = read.flatten().collect();
        // Ordre déterministe (tests, et affichage stable si jamais aucune
        // date fiable n'est disponible sur le système de fichiers).
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let paths = Paths::new(&path);
            if !paths.has_identity() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            let stamp = file_time_ms(&paths.vault());
            out.push(AccountEntry {
                dir_name: format!("{PROFILES_DIR_NAME}/{id}"),
                name: format!("{DISCOVERED_NAME_PREFIX}{}", short_id(&id)),
                id,
                created_ms: stamp,
                last_used_ms: stamp,
                pubkey_hex: None,
            });
        }
        out
    }

    /// Entrée du profil historique, avec un horodatage best-effort tiré du
    /// coffre sur disque (plutôt que l'instant présent) quand disponible.
    fn legacy_entry(&self) -> AccountEntry {
        let stamp = file_time_ms(&self.legacy_paths().vault());
        AccountEntry {
            id: LEGACY_ID.to_string(),
            dir_name: LEGACY_DIR_NAME.to_string(),
            name: LEGACY_PLACEHOLDER_NAME.to_string(),
            created_ms: stamp,
            last_used_ms: stamp,
            pubkey_hex: None,
        }
    }

    /// Écrit `profiles.json` en entier (pas d'écriture partielle ni de
    /// suppression : un remplacement complet et cohérent du contenu connu).
    fn write_locked(&self, accounts: &[AccountEntry]) -> Result<(), NodeError> {
        let file = RegistryFile {
            schema_version: SCHEMA_VERSION,
            accounts: accounts.to_vec(),
        };
        let bytes = serde_json::to_vec_pretty(&file).map_err(std::io::Error::other)?;
        if let Some(parent) = self.registry_path().parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(self.registry_path(), bytes)?;
        Ok(())
    }

    /// Chemin du fichier de registre.
    fn registry_path(&self) -> PathBuf {
        self.root.join(REGISTRY_FILE)
    }
}

/// Génère un identifiant de compte aléatoire (128 bits, CSPRNG, hexadécimal)
/// — unique en pratique sans dépendance à un format UUID particulier, sur le
/// même principe que [`accord_api::AuthToken::generate`] pour les jetons de
/// session.
fn generate_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(&bytes)
}

/// Préfixe court et lisible d'un identifiant, pour un nom provisoire.
fn short_id(id: &str) -> &str {
    &id[..id.len().min(8)]
}

/// Date de dernière modification d'un fichier, en millisecondes Unix
/// best-effort (horloge murale locale à défaut — jamais bloquant).
fn file_time_ms(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or_else(now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Difficulté PoW réduite pour des tests rapides.
    const POW_TEST: u32 = 1;

    #[test]
    fn create_read_update_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::new(dir.path());

        assert!(registry.load_or_init().unwrap().is_empty());

        let (draft, paths) = registry.new_entry("Alice");
        seal_identity(&paths);
        let id = draft.id.clone();
        registry.register(draft).unwrap();

        let loaded = registry.get(&id).unwrap().expect("compte enregistré");
        assert_eq!(loaded.name, "Alice");
        assert_eq!(loaded.dir_name, format!("{PROFILES_DIR_NAME}/{id}"));
        assert_eq!(loaded.created_ms, loaded.last_used_ms);

        // `record_use` avance `last_used_ms` et peut rafraîchir le nom / la
        // clé publique, sans jamais effacer un champ par une valeur absente.
        std::thread::sleep(std::time::Duration::from_millis(2));
        registry
            .record_use(&id, Some("Alice B.".into()), Some("ab12".into()))
            .unwrap();
        let refreshed = registry.get(&id).unwrap().unwrap();
        assert_eq!(refreshed.name, "Alice B.");
        assert_eq!(refreshed.pubkey_hex.as_deref(), Some("ab12"));
        assert!(refreshed.last_used_ms >= loaded.last_used_ms);

        registry.record_use(&id, None, None).unwrap();
        let unchanged_name = registry.get(&id).unwrap().unwrap();
        assert_eq!(
            unchanged_name.name, "Alice B.",
            "un nom absent ne doit rien effacer"
        );

        let listed = registry.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, id);
    }

    #[test]
    fn corrupt_registry_file_rebuilds_from_scan_without_data_loss() {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::new(dir.path());

        // Un profil historique et un profil "nouveau style" existent tous
        // deux réellement sur disque.
        seal_identity(&registry.legacy_paths());
        let (draft, paths) = registry.new_entry("Bob");
        seal_identity(&paths);
        registry.register(draft).unwrap();

        // Le fichier de registre est ensuite corrompu (E/S, coupure de
        // courant en cours d'écriture, etc.).
        std::fs::write(dir.path().join(REGISTRY_FILE), b"{ pas du json valide").unwrap();

        let accounts = registry.load_or_init().unwrap();
        assert_eq!(accounts.len(), 2, "aucun des deux comptes n'est perdu");
        assert!(accounts.iter().any(|a| a.id == LEGACY_ID));
        assert!(accounts
            .iter()
            .any(|a| a.dir_name.starts_with(PROFILES_DIR_NAME)));

        // Le fichier corrompu a été remplacé par une version valide : un
        // second chargement n'a plus rien à corriger.
        let reloaded = registry.load_or_init().unwrap();
        assert_eq!(reloaded.len(), 2);
    }

    #[test]
    fn legacy_dir_auto_registered_exactly_once_idempotent_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        seal_identity(&Paths::new(dir.path().join(LEGACY_DIR_NAME)));

        // Premier "démarrage" : aucun profiles.json n'existe encore.
        let first_boot = Registry::new(dir.path());
        let accounts = first_boot.load_or_init().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, LEGACY_ID);
        assert!(dir.path().join(REGISTRY_FILE).exists());

        // Redémarrages suivants (nouvelles instances de `Registry`, comme un
        // relancement de l'application) : toujours une seule entrée.
        for _ in 0..3 {
            let boot = Registry::new(dir.path());
            let accounts = boot.load_or_init().unwrap();
            assert_eq!(
                accounts.len(),
                1,
                "la migration ne doit jamais dupliquer l'entrée"
            );
            assert_eq!(accounts[0].id, LEGACY_ID);
        }
    }

    #[test]
    fn record_use_on_unknown_non_legacy_id_is_ignored_without_fabricating_an_entry() {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::new(dir.path());
        registry
            .record_use("ne-existe-pas", Some("Mallory".into()), None)
            .unwrap();
        assert!(registry.get("ne-existe-pas").unwrap().is_none());
    }

    #[test]
    fn most_recently_used_picks_the_highest_last_used_ms() {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::new(dir.path());

        let (a, paths_a) = registry.new_entry("A");
        seal_identity(&paths_a);
        let id_a = a.id.clone();
        registry.register(a).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(2));
        let (b, paths_b) = registry.new_entry("B");
        seal_identity(&paths_b);
        let id_b = b.id.clone();
        registry.register(b).unwrap();

        assert_eq!(registry.most_recently_used().unwrap().unwrap().id, id_b);

        registry.record_use(&id_a, None, None).unwrap();
        assert_eq!(registry.most_recently_used().unwrap().unwrap().id, id_a);
    }

    /// Scelle une identité minimale sur `paths`, pour que `has_identity()`
    /// soit vrai (les tests de ce module portent sur le registre, pas sur le
    /// scellement lui-même — voir `identity::tests` pour ce dernier).
    fn seal_identity(paths: &Paths) {
        crate::identity::create(paths, "phrase-de-passe-test", POW_TEST).unwrap();
    }
}
