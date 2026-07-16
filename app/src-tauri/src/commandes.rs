//! Commandes Tauri : contrat exact attendu par `app/src/lib/bridge.ts`.
//!
//! Les commandes de cycle de vie (création, restauration, déverrouillage)
//! effectuent un travail CPU lourd (PoW d'identité, Argon2id) : elles sont
//! asynchrones et délèguent ce travail à un fil bloquant pour ne jamais
//! geler le fil principal de la fenêtre.

use accord_node::{identity, NodeConfig, Unlocked};
use tauri::State;

use crate::erreur::ErreurHote;
use crate::etat::{
    CompteCree, CompteMeta, CompteRestaure, EtatHote, IdentiteCreee, InfoSession, StatutCoffre,
};

/// Difficulté PoW des identités (SPEC §2.2).
const POW_BITS: u32 = accord_proto::limits::IDENTITY_POW_BITS;

/// Nom provisoire d'un compte fraîchement créé, avant que son pseudo public
/// (`profile.set`) n'existe : rafraîchi automatiquement au premier
/// déverrouillage qui suit sa définition (voir
/// `EtatHote::rafraichir_compte_actif`).
const NOUVEAU_COMPTE_NOM_PROVISOIRE: &str = "Nouveau compte";

/// Statut du coffre d'identité : `'absent'` ou `'locked'`.
#[tauri::command]
pub fn vault_status(etat: State<'_, EtatHote>) -> StatutCoffre {
    etat.statut_coffre()
}

/// Quitte complètement l'application (déclenche `RunEvent::Exit`, donc l'arrêt
/// propre du nœud). Appelée par l'interception de fermeture de la fenêtre
/// quand « réduire dans la barre des menus » est désactivé : laisser le
/// comportement par défaut de la plateforme ne quitte PAS l'app sur macOS
/// (fermer la fenêtre y garde le process et le nœud en vie), d'où une sortie
/// explicite et cohérente sur toutes les plateformes. « Quitter » depuis la
/// tray suit le même chemin (`app.exit(0)`, voir `tray.rs`).
#[tauri::command]
pub fn app_quit(app: tauri::AppHandle) {
    app.exit(0);
}

/// Crée une identité neuve (PoW + scellement), démarre le nœud et rend la
/// session ainsi que la phrase de récupération à faire noter.
#[tauri::command]
pub async fn create_identity(
    etat: State<'_, EtatHote>,
    passphrase: String,
) -> Result<IdentiteCreee, ErreurHote> {
    let chemins = etat.chemins();
    let (deverrouille, phrase) =
        en_arriere_plan(move || identity::create_with_phrase(&chemins, &passphrase, POW_BITS))
            .await?;
    let session = demarrer(&etat, deverrouille).await?;
    Ok(IdentiteCreee {
        session,
        recovery_phrase: (*phrase).clone(),
    })
}

/// Restaure une identité depuis sa phrase de récupération, la scelle sous la
/// nouvelle phrase de passe locale, puis démarre le nœud.
#[tauri::command]
pub async fn restore_identity(
    etat: State<'_, EtatHote>,
    phrase: String,
    passphrase: String,
) -> Result<InfoSession, ErreurHote> {
    let chemins = etat.chemins();
    let deverrouille = en_arriere_plan(move || {
        identity::restore_from_phrase(&chemins, &phrase, &passphrase, POW_BITS)
    })
    .await?;
    demarrer(&etat, deverrouille).await
}

/// Déverrouille le coffre existant puis démarre le nœud.
#[tauri::command]
pub async fn unlock(
    etat: State<'_, EtatHote>,
    passphrase: String,
) -> Result<InfoSession, ErreurHote> {
    let chemins = etat.chemins();
    let deverrouille = en_arriere_plan(move || identity::unlock(&chemins, &passphrase)).await?;
    demarrer(&etat, deverrouille).await
}

/// Liste les comptes locaux connus (contrat `AccountMeta[]`), du plus
/// récemment utilisé au moins récent — de quoi peupler un sélecteur de
/// comptes avant tout déverrouillage. Ne révèle jamais de secret : seules
/// les métadonnées du registre (voir `accord_node::registry`).
#[tauri::command]
pub fn accounts_list(etat: State<'_, EtatHote>) -> Result<Vec<CompteMeta>, ErreurHote> {
    Ok(etat
        .registre()
        .list()?
        .into_iter()
        .map(CompteMeta::from)
        .collect())
}

/// Crée un compte **neuf** (répertoire de profil dédié, distinct de tout
/// compte existant), démarre son nœud et rend la session ainsi que la
/// phrase de récupération à faire noter. Symétrique de `create_identity`,
/// mais jamais sur le profil actif courant : le compte n'est enregistré
/// dans le registre qu'après succès du scellement de l'identité, pour ne
/// jamais y référencer un répertoire vide en cas d'échec.
#[tauri::command]
pub async fn account_create(
    etat: State<'_, EtatHote>,
    passphrase: String,
) -> Result<CompteCree, ErreurHote> {
    let (brouillon, chemins) = etat.registre().new_entry(NOUVEAU_COMPTE_NOM_PROVISOIRE);
    let id = brouillon.id.clone();
    let (deverrouille, phrase) = en_arriere_plan({
        let chemins = chemins.clone();
        move || identity::create_with_phrase(&chemins, &passphrase, POW_BITS)
    })
    .await?;
    etat.registre().register(brouillon)?;
    etat.activer(id.clone(), chemins);
    let session = demarrer(&etat, deverrouille).await?;
    Ok(CompteCree {
        session,
        recovery_phrase: (*phrase).clone(),
        account_id: id,
    })
}

/// Restaure un compte **neuf** depuis sa phrase de récupération (jamais sur
/// le profil actif courant), le scelle sous la nouvelle phrase de passe
/// locale, puis démarre son nœud. Même discipline d'enregistrement tardif
/// que `account_create` : le compte n'existe dans le registre qu'une fois
/// son identité effectivement scellée sur disque.
#[tauri::command]
pub async fn account_restore(
    etat: State<'_, EtatHote>,
    phrase: String,
    passphrase: String,
) -> Result<CompteRestaure, ErreurHote> {
    let (brouillon, chemins) = etat.registre().new_entry(NOUVEAU_COMPTE_NOM_PROVISOIRE);
    let id = brouillon.id.clone();
    let deverrouille = en_arriere_plan({
        let chemins = chemins.clone();
        move || identity::restore_from_phrase(&chemins, &phrase, &passphrase, POW_BITS)
    })
    .await?;
    etat.registre().register(brouillon)?;
    etat.activer(id.clone(), chemins);
    let session = demarrer(&etat, deverrouille).await?;
    Ok(CompteRestaure {
        session,
        account_id: id,
    })
}

/// Déverrouille un compte existant du registre et bascule dessus : arrête
/// l'éventuel nœud actif (autre compte) avant de démarrer celui-ci — même
/// primitive que `demarrer` utilise déjà pour le profil fixe historique, il
/// n'existe pas de chemin de bascule séparé. Le profil actif n'est changé
/// qu'après succès du déverrouillage : une phrase de passe incorrecte ne
/// perturbe jamais la session en cours.
#[tauri::command]
pub async fn account_unlock(
    etat: State<'_, EtatHote>,
    account_id: String,
    passphrase: String,
) -> Result<InfoSession, ErreurHote> {
    let compte = etat
        .registre()
        .get(&account_id)?
        .ok_or(accord_node::NodeError::NotFound("compte"))?;
    let chemins = etat.registre().paths_of(&compte);
    let deverrouille = en_arriere_plan({
        let chemins = chemins.clone();
        move || identity::unlock(&chemins, &passphrase)
    })
    .await?;
    etat.activer(compte.id, chemins);
    demarrer(&etat, deverrouille).await
}

/// Ferme la session courante : arrête le nœud actif et ramène l'UI à
/// l'écran d'accueil (sélecteur de comptes), sans changer le profil actif
/// ni rien effacer sur disque. Distinct de `lock` seulement par intention —
/// `lock` verrouille *ce* compte, `session_close` quitte vers le
/// sélecteur ; les deux se résument aujourd'hui à `arreter_noeud`, la
/// distinction sert de point d'extension si leurs comportements divergent
/// plus tard.
#[tauri::command]
pub async fn session_close(etat: State<'_, EtatHote>) -> Result<StatutCoffre, ErreurHote> {
    etat.arreter_noeud();
    Ok(etat.statut_coffre())
}

/// Locks the vault without quitting the app: the exact inverse of `unlock`.
///
/// Stops the running node (network, API, database) and drops it; the
/// in-memory secrets (`Unlocked` seed, SQLCipher key) are `Zeroizing` and are
/// wiped on that drop. Returns the fresh vault status so the UI lands on the
/// same screen as a cold start (`"locked"`, or `"absent"` if the vault file
/// disappeared meanwhile). Async so the WebView thread never blocks on the
/// node shutdown; infallible in practice but typed as `Result` because async
/// Tauri commands borrowing `State` require it.
#[tauri::command]
pub async fn lock(etat: State<'_, EtatHote>) -> Result<StatutCoffre, ErreurHote> {
    etat.arreter_noeud();
    Ok(etat.statut_coffre())
}

/// État de l'autorisation micro : `granted` / `denied` / `undetermined` /
/// `restricted` / `unsupported` (plateformes sans TCC : toujours
/// `unsupported`, l'UI n'affiche alors pas d'état). Ne déclenche jamais
/// l'invite — c'est le point : l'UI ne redemande qu'à l'état indéterminé,
/// jamais en boucle (voir `accord-macos`).
#[tauri::command]
pub fn micro_autorisation_etat() -> &'static str {
    accord_macos::micro_etat()
}

/// Déclenche l'invite micro système (utile à l'état « indéterminé » seulement
/// — sinon l'OS répond immédiatement sans invite). Rend l'issue. L'attente de
/// la réponse utilisateur est bloquante : fil dédié, jamais le fil principal.
#[tauri::command]
pub async fn micro_autorisation_demander() -> Result<bool, ErreurHote> {
    tauri::async_runtime::spawn_blocking(accord_macos::micro_demander_bloquant)
        .await
        .map_err(|e| ErreurHote::Tache(e.to_string()))?
        .map_err(ErreurHote::Tache)
}

/// Ouvre le panneau des réglages système correspondant à une autorisation.
///
/// Après un refus (micro, notifications) l'OS ne ré-affiche plus jamais son
/// invite : le seul recours de l'utilisateur est le panneau système, d'où ce
/// raccourci. `section` est validée contre une liste fermée — jamais d'URL
/// arbitraire. Le pare-feu y figure car Accord est P2P : sans acceptation des
/// connexions ENTRANTES, un pair ne peut pas nous joindre directement.
#[tauri::command]
pub fn ouvrir_reglages_systeme(section: String) -> Result<(), ErreurHote> {
    #[cfg(target_os = "macos")]
    let cible = match section.as_str() {
        "microphone" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        "notifications" => "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
        "firewall" => "x-apple.systempreferences:com.apple.Network-Settings.extension?Firewall",
        _ => return Err(ErreurHote::Tache("section inconnue".into())),
    };
    #[cfg(target_os = "windows")]
    let cible = match section.as_str() {
        "microphone" => "ms-settings:privacy-microphone",
        "notifications" => "ms-settings:notifications",
        "firewall" => "windowsdefender://network/",
        _ => return Err(ErreurHote::Tache("section inconnue".into())),
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let _ = section;
        return Err(ErreurHote::Tache(
            "réglages système non pris en charge sur cette plateforme".into(),
        ));
    }
    #[cfg(target_os = "macos")]
    let lancement = std::process::Command::new("open").arg(cible).spawn();
    #[cfg(target_os = "windows")]
    let lancement = std::process::Command::new("cmd")
        .args(["/C", "start", "", cible])
        .spawn();
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    match lancement {
        Ok(_) => Ok(()),
        Err(e) => Err(ErreurHote::Tache(format!("ouverture des réglages : {e}"))),
    }
}

/// Exécute un travail CPU lourd hors du fil principal.
async fn en_arriere_plan<T, F>(travail: F) -> Result<T, ErreurHote>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, accord_node::NodeError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(travail)
        .await
        .map_err(|e| ErreurHote::Tache(e.to_string()))?
        .map_err(ErreurHote::from)
}

/// Arrête l'éventuel nœud courant puis en démarre un neuf sur le profil :
/// API locale sur port éphémère, mais UDP P2P sur port stable (B2) — le port
/// `0` de `p2p_addr` déclenche la stratégie de port stable (port retenu au
/// précédent lancement, sinon 48016 et plage de repli), pour qu'un ami puisse
/// joindre une adresse `ip:port` prévisible.
async fn demarrer(etat: &EtatHote, deverrouille: Unlocked) -> Result<InfoSession, ErreurHote> {
    etat.arreter_noeud();
    // `NodeConfig::default()` fixe déjà `p2p_addr = 0.0.0.0:0`, c'est-à-dire la
    // stratégie de port stable (et non un port réellement aléatoire).
    let config = NodeConfig {
        paths: etat.chemins(),
        // Rendez-vous partagé du premier contact (ACCORD_BOOTSTRAP) : sans lui,
        // deux amis tous deux derrière un NAT symétrique ne peuvent pas se
        // joindre (aucun n'est joignable).
        default_bootstrap: accord_node::default_bootstrap_env(),
        ..NodeConfig::default()
    };
    let noeud = accord_node::run(deverrouille, config).await?;
    let session = etat.installer_noeud(noeud);
    // Rafraîchit le nom affiché et `last_used_ms` du compte actif dans le
    // registre — s'applique uniformément aux commandes historiques
    // (`create_identity`, `restore_identity`, `unlock`) et aux nouvelles
    // (`account_create`, `account_restore`, `account_unlock`) puisqu'elles
    // passent toutes par ce même point de démarrage.
    etat.rafraichir_compte_actif();
    Ok(session)
}
