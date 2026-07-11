//! État partagé de l'hôte : registre de comptes, profil actif et nœud en
//! cours d'exécution.
//!
//! Le profil actif est désormais **mutable** (multi-comptes, D-046) : au
//! lieu d'un unique `PathBuf` fixé au démarrage, l'état retient l'identifiant
//! et les chemins du compte courant, rebasculables par
//! [`EtatHote::activer`]. Les commandes historiques (`vault_status`,
//! `create_identity`, `restore_identity`, `unlock`, `lock`) n'ont pas changé
//! de signature : elles continuent d'agir via [`EtatHote::chemins`], qui
//! rend maintenant les chemins du compte *actif* plutôt qu'un unique
//! répertoire fixe — c'est ce qui leur permet de continuer à fonctionner
//! sans modification une fois le multi-compte câblé.

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use accord_node::registry::{AccountEntry, Registry, LEGACY_ID};
use accord_node::{NodeError, Paths, RunningNode};
use serde::Serialize;

/// Statut du coffre d'identité, tel qu'attendu par `bridge.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum StatutCoffre {
    /// Aucune identité sur disque : l'UI propose création ou restauration.
    #[serde(rename = "absent")]
    Absent,
    /// Un coffre existe : l'UI demande la phrase de passe.
    #[serde(rename = "locked")]
    Verrouille,
}

/// Session de l'API locale à transmettre à l'UI (contrat `SessionInfo`).
#[derive(Debug, Serialize)]
pub struct InfoSession {
    /// Port TCP de l'API WebSocket locale.
    pub port: u16,
    /// Jeton d'authentification de l'API.
    pub token: String,
}

/// Résultat de la création d'identité (contrat `CreatedIdentity`).
#[derive(Debug, Serialize)]
pub struct IdentiteCreee {
    /// Session du nœud fraîchement démarré.
    pub session: InfoSession,
    /// Phrase de récupération de 12 mots — affichée une seule fois.
    pub recovery_phrase: String,
}

/// Compte tel qu'exposé au sélecteur de comptes (contrat `AccountMeta`).
/// Métadonnées d'affichage uniquement : jamais de secret (voir
/// `accord_node::registry` pour le compromis « nom en clair »).
#[derive(Debug, Clone, Serialize)]
pub struct CompteMeta {
    /// Identifiant stable du compte.
    pub id: String,
    /// Nom affiché.
    pub name: String,
    /// Date de création estimée (millisecondes Unix).
    pub created_ms: u64,
    /// Date de dernière utilisation (millisecondes Unix).
    pub last_used_ms: u64,
    /// Vrai pour le profil historique (unique compte pré-multi-compte).
    pub is_legacy: bool,
    /// Préfixe court (8 caractères hex) de la clé publique, si connue —
    /// désambiguïsation de deux comptes de même nom affiché.
    pub pubkey_short: Option<String>,
}

impl From<AccountEntry> for CompteMeta {
    fn from(entree: AccountEntry) -> Self {
        Self {
            is_legacy: entree.id == LEGACY_ID,
            pubkey_short: entree
                .pubkey_hex
                .as_deref()
                .map(|h| h.chars().take(8).collect()),
            id: entree.id,
            name: entree.name,
            created_ms: entree.created_ms,
            last_used_ms: entree.last_used_ms,
        }
    }
}

/// Résultat de la création d'un nouveau compte (contrat `AccountCreated`).
#[derive(Debug, Serialize)]
pub struct CompteCree {
    /// Session du nœud fraîchement démarré.
    pub session: InfoSession,
    /// Phrase de récupération de 12 mots — affichée une seule fois.
    pub recovery_phrase: String,
    /// Identifiant du compte créé.
    pub account_id: String,
}

/// Résultat de la restauration d'un compte depuis sa phrase de récupération
/// (contrat `AccountRestored`). Pas de `recovery_phrase` : contrairement à
/// la création, la restauration ne fait pas naître de nouvelle phrase — la
/// personne utilise déjà celle qu'elle a saisie (même contrat que l'actuel
/// `restore_identity`, `account_id` en plus).
#[derive(Debug, Serialize)]
pub struct CompteRestaure {
    /// Session du nœud fraîchement démarré.
    pub session: InfoSession,
    /// Identifiant du compte restauré.
    pub account_id: String,
}

/// Statut du coffre pour un profil donné (logique pure, testable sans Tauri).
pub fn statut_du_coffre(chemins: &Paths) -> StatutCoffre {
    if chemins.has_identity() {
        StatutCoffre::Verrouille
    } else {
        StatutCoffre::Absent
    }
}

/// Profil actif : identifiant de compte + chemins résolus.
struct CompteActif {
    id: String,
    chemins: Paths,
}

/// État partagé géré par Tauri : registre de comptes, profil actif et nœud
/// courant. Un seul nœud tourne à la fois, quel que soit le nombre de
/// comptes enregistrés (voir [`EtatHote::installer_noeud`]).
pub struct EtatHote {
    /// Registre des comptes locaux (métadonnées en clair, jamais de secret).
    registre: Registry,
    /// Profil actif (celui que les commandes historiques manipulent).
    actif: Mutex<CompteActif>,
    /// Nœud en cours d'exécution, s'il y en a un.
    noeud: Mutex<Option<RunningNode>>,
}

impl EtatHote {
    /// Construit l'état pour un unique répertoire de profil fixe
    /// (rétrocompatibilité et tests unitaires légers) : le registre est
    /// raciné sur son répertoire parent mais n'est consulté que si les
    /// commandes multi-comptes sont explicitement appelées.
    pub fn new(profil: PathBuf) -> Self {
        let racine = profil
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| profil.clone());
        Self {
            registre: Registry::new(racine),
            actif: Mutex::new(CompteActif {
                id: LEGACY_ID.to_string(),
                chemins: Paths::new(profil),
            }),
            noeud: Mutex::new(None),
        }
    }

    /// Construit l'état depuis le répertoire de données de l'application :
    /// charge/migre le registre (voir `Registry::load_or_init`) puis active
    /// le compte le plus récemment utilisé, ou le profil historique si le
    /// registre est encore vide (installation neuve, ou tout premier
    /// lancement avant toute identité créée).
    pub fn depuis_repertoire_app(app_data_dir: PathBuf) -> Result<Self, NodeError> {
        let registre = Registry::new(app_data_dir);
        let comptes = registre.load_or_init()?;
        let actif = match comptes.into_iter().max_by_key(|c| c.last_used_ms) {
            Some(compte) => CompteActif {
                chemins: registre.paths_of(&compte),
                id: compte.id,
            },
            None => CompteActif {
                chemins: registre.legacy_paths(),
                id: LEGACY_ID.to_string(),
            },
        };
        Ok(Self {
            registre,
            actif: Mutex::new(actif),
            noeud: Mutex::new(None),
        })
    }

    /// Chemins du profil **actif** (coffre + base) — utilisé par toutes les
    /// commandes de cycle de vie, historiques comme multi-comptes.
    pub fn chemins(&self) -> Paths {
        self.actif
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .chemins
            .clone()
    }

    /// Identifiant du compte actif.
    pub fn id_actif(&self) -> String {
        self.actif
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .id
            .clone()
    }

    /// Registre des comptes locaux.
    pub fn registre(&self) -> &Registry {
        &self.registre
    }

    /// Bascule le profil actif vers un autre compte. Ne touche pas au nœud
    /// en cours : c'est `demarrer` (arrêt puis démarrage, voir
    /// `commandes.rs`) qui applique la bascule effective, exactement comme
    /// pour le profil fixe historique — aucune primitive de commutation
    /// séparée n'est nécessaire.
    pub fn activer(&self, id: String, chemins: Paths) {
        *self.actif.lock().unwrap_or_else(PoisonError::into_inner) = CompteActif { id, chemins };
    }

    /// Statut actuel du coffre d'identité (profil actif).
    pub fn statut_coffre(&self) -> StatutCoffre {
        statut_du_coffre(&self.chemins())
    }

    /// Arrête et libère le nœud courant, s'il existe.
    pub fn arreter_noeud(&self) {
        let pris = self
            .noeud
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(noeud) = pris {
            noeud.shutdown();
        }
    }

    /// Installe un nœud fraîchement démarré et rend sa session pour l'UI.
    /// Si un nœud tournait encore (course improbable, ou bascule de
    /// compte), il est arrêté : un seul nœud actif à la fois, quel que soit
    /// le profil.
    pub fn installer_noeud(&self, noeud: RunningNode) -> InfoSession {
        let info = InfoSession {
            port: noeud.api_addr().port(),
            token: noeud.token.expose().to_owned(),
        };
        let remplace = self
            .noeud
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(noeud);
        if let Some(ancien) = remplace {
            ancien.shutdown();
        }
        info
    }

    /// Relit le pseudo et la clé publique du nœud actif (s'il en a un et si
    /// un pseudo a déjà été défini via `profile.set`) pour rafraîchir le
    /// registre : nom affiché du compte actif + date de dernière
    /// utilisation. Best-effort et jamais fatal — une erreur d'écriture du
    /// registre ne doit jamais faire échouer un déverrouillage par ailleurs
    /// réussi.
    pub fn rafraichir_compte_actif(&self) {
        let id = self.id_actif();
        let profil = self
            .noeud
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .and_then(|n| n.node.self_profile().ok());
        let (nom, pubkey) = match profil {
            Some(p) => (p.name, Some(p.pubkey)),
            None => (None, None),
        };
        if let Err(e) = self.registre.record_use(&id, nom, pubkey) {
            tracing::warn!(erreur = %e, "mise à jour du registre de comptes impossible");
        }
    }

    /// Port de l'API du nœud actuellement installé, s'il y en a un — pour
    /// vérifier en test qu'une bascule de compte ne laisse jamais deux
    /// nœuds installés (l'`Option` interdit déjà l'accumulation ; ceci
    /// vérifie en plus *lequel* des deux nœuds démarrés reste actif).
    #[cfg(test)]
    fn port_noeud_actif(&self) -> Option<u16> {
        self.noeud
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|n| n.api_addr().port())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Difficulté PoW réduite pour des tests rapides.
    const POW_TEST: u32 = 1;

    #[test]
    fn statut_absent_sans_coffre_puis_verrouille_apres_creation() {
        let dossier = tempfile::tempdir().unwrap();
        let chemins = Paths::new(dossier.path());
        assert_eq!(statut_du_coffre(&chemins), StatutCoffre::Absent);

        accord_node::identity::create(&chemins, "phrase-de-passe", POW_TEST).unwrap();
        assert_eq!(statut_du_coffre(&chemins), StatutCoffre::Verrouille);
    }

    #[test]
    fn statut_coffre_serialise_selon_le_contrat_du_pont() {
        // `bridge.ts` attend exactement 'absent' | 'locked'.
        assert_eq!(
            serde_json::to_value(StatutCoffre::Absent).unwrap(),
            serde_json::json!("absent")
        );
        assert_eq!(
            serde_json::to_value(StatutCoffre::Verrouille).unwrap(),
            serde_json::json!("locked")
        );
    }

    #[test]
    fn lock_without_running_node_is_idempotent_and_keeps_vault_locked() {
        // The `lock` command boils down to `arreter_noeud` + `statut_coffre`:
        // with no node running it must be a harmless no-op, and the vault on
        // disk must still report `locked` so the UI lands on the unlock
        // screen exactly like a fresh launch.
        let dossier = tempfile::tempdir().unwrap();
        accord_node::identity::create(&Paths::new(dossier.path()), "phrase-de-passe", POW_TEST)
            .unwrap();
        let etat = EtatHote::new(dossier.path().to_path_buf());

        etat.arreter_noeud();
        etat.arreter_noeud();

        assert_eq!(etat.statut_coffre(), StatutCoffre::Verrouille);
    }

    #[test]
    fn identite_creee_serialise_selon_le_contrat_du_pont() {
        // `bridge.ts` attend { session: { port, token }, recovery_phrase }.
        let cree = IdentiteCreee {
            session: InfoSession {
                port: 4242,
                token: "jeton".into(),
            },
            recovery_phrase: "douze mots".into(),
        };
        let json = serde_json::to_value(&cree).unwrap();
        assert_eq!(json["session"]["port"], 4242);
        assert_eq!(json["session"]["token"], "jeton");
        assert_eq!(json["recovery_phrase"], "douze mots");
    }

    #[test]
    fn compte_meta_serialise_avec_pubkey_courte_et_marqueur_legacy() {
        let entree = AccountEntry {
            id: LEGACY_ID.to_string(),
            dir_name: "profil".into(),
            name: "Mon compte".into(),
            created_ms: 1,
            last_used_ms: 2,
            pubkey_hex: Some("abcdef0123456789".into()),
        };
        let meta = CompteMeta::from(entree);
        assert!(meta.is_legacy);
        assert_eq!(meta.pubkey_short.as_deref(), Some("abcdef01"));

        let json = serde_json::to_value(&meta).unwrap();
        assert_eq!(json["id"], LEGACY_ID);
        assert_eq!(json["name"], "Mon compte");
        assert_eq!(json["is_legacy"], true);
    }

    #[test]
    fn depuis_repertoire_app_active_le_profil_historique_si_registre_vide() {
        let app_dir = tempfile::tempdir().unwrap();
        // Aucune identité nulle part : premier lancement d'une installation
        // neuve. Le profil actif par défaut doit rester le profil
        // historique, pour que `create_identity` continue de fonctionner
        // exactement comme avant le multi-compte.
        let etat = EtatHote::depuis_repertoire_app(app_dir.path().to_path_buf()).unwrap();
        assert_eq!(etat.id_actif(), LEGACY_ID);
        assert_eq!(etat.chemins().root, app_dir.path().join("profil"));
    }

    #[test]
    fn depuis_repertoire_app_active_le_compte_le_plus_recemment_utilise() {
        let app_dir = tempfile::tempdir().unwrap();
        let registre = accord_node::registry::Registry::new(app_dir.path().to_path_buf());

        let (a, chemins_a) = registre.new_entry("A");
        accord_node::identity::create(&chemins_a, "phrase-a", POW_TEST).unwrap();
        let id_a = a.id.clone();
        registre.register(a).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(2));
        let (b, chemins_b) = registre.new_entry("B");
        accord_node::identity::create(&chemins_b, "phrase-b", POW_TEST).unwrap();
        let id_b = b.id.clone();
        registre.register(b).unwrap();

        let etat = EtatHote::depuis_repertoire_app(app_dir.path().to_path_buf()).unwrap();
        assert_eq!(etat.id_actif(), id_b, "B a été créé/utilisé en dernier");

        registre.record_use(&id_a, None, None).unwrap();
        let etat2 = EtatHote::depuis_repertoire_app(app_dir.path().to_path_buf()).unwrap();
        assert_eq!(etat2.id_actif(), id_a, "A vient d'être réutilisé");
    }

    #[test]
    fn activer_bascule_les_chemins_et_lidentifiant_actifs() {
        let dossier = tempfile::tempdir().unwrap();
        let etat = EtatHote::new(dossier.path().join("profil"));
        assert_eq!(etat.id_actif(), LEGACY_ID);

        let autre = Paths::new(dossier.path().join("profiles/autre"));
        etat.activer("autre".to_string(), autre.clone());
        assert_eq!(etat.id_actif(), "autre");
        assert_eq!(etat.chemins().root, autre.root);
    }

    /// Bascule de compte : le nœud du compte A ne doit jamais rester
    /// installé une fois celui de B en place — au plus un nœud actif à la
    /// fois, quel que soit le profil. Vérifié directement sur l'état
    /// interne d'`EtatHote` (port du nœud effectivement installé), sans
    /// délai arbitraire ni sondage réseau.
    #[tokio::test]
    async fn basculer_de_compte_remplace_le_noeud_actif_sans_jamais_en_accumuler_deux() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        let chemins_a = Paths::new(dir_a.path());
        let chemins_b = Paths::new(dir_b.path());
        let deverrouille_a =
            accord_node::identity::create(&chemins_a, "phrase-a", POW_TEST).unwrap();
        let deverrouille_b =
            accord_node::identity::create(&chemins_b, "phrase-b", POW_TEST).unwrap();

        let config = |chemins: Paths| accord_node::NodeConfig {
            paths: chemins,
            p2p_addr: "127.0.0.1:0".parse().unwrap(),
            pow_bits: POW_TEST,
            ..accord_node::NodeConfig::default()
        };

        let etat = EtatHote::new(dir_a.path().to_path_buf());
        assert_eq!(etat.port_noeud_actif(), None);

        let noeud_a = accord_node::run(deverrouille_a, config(chemins_a))
            .await
            .unwrap();
        let session_a = etat.installer_noeud(noeud_a);
        assert_eq!(etat.port_noeud_actif(), Some(session_a.port));

        // Bascule vers B : même discipline que `demarrer` (arrêt de
        // l'actif puis installation du nouveau) — reproduite ici sans
        // passer par la commande Tauri, pour rester un test unitaire pur.
        etat.activer("b".to_string(), chemins_b.clone());
        etat.arreter_noeud();
        assert_eq!(
            etat.port_noeud_actif(),
            None,
            "aucun nœud ne doit rester installé entre l'arrêt de A et le démarrage de B"
        );

        let noeud_b = accord_node::run(deverrouille_b, config(chemins_b))
            .await
            .unwrap();
        let session_b = etat.installer_noeud(noeud_b);

        assert_ne!(session_a.port, session_b.port);
        assert_eq!(
            etat.port_noeud_actif(),
            Some(session_b.port),
            "seul le nœud de B doit rester installé après la bascule"
        );
        assert_eq!(etat.id_actif(), "b");

        etat.arreter_noeud();
    }
}
