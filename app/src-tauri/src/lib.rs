//! Hôte de bureau Tauri d'Accord.
//!
//! Rôle : porter le cycle de vie de l'identité (création, restauration,
//! déverrouillage) **hors** du canal JSON-RPC de l'API locale — ces
//! opérations manipulent des secrets et précèdent l'existence même du nœud —
//! puis démarrer le nœud embarqué (`accord-node`) et transmettre à l'UI le
//! couple `{ port, token }` de l'API WebSocket locale.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod commandes;
pub mod erreur;
pub mod etat;
pub mod journal;
pub mod tray;

use std::process::ExitCode;

use tauri::Manager;

use etat::EtatHote;

/// Construit puis lance l'application Tauri. Rend un code de sortie explicite
/// au lieu de paniquer en cas d'échec de démarrage.
pub fn executer() -> ExitCode {
    // Journal sur disque, dans `<app_data>/logs` (§10.6). Le dossier n'est
    // connu qu'au `setup` ; les premières lignes sont gardées en mémoire d'ici
    // là — voir `journal`.
    let journal = journal::init();

    let application = tauri::Builder::default()
        // Notifications système natives (D-028) : l'envoi se fait côté
        // webview via @tauri-apps/plugin-notification.
        .plugin(tauri_plugin_notification::init())
        // Lancement au démarrage du système, état géré par l'OS
        // (Registre Windows / LaunchAgent macOS / fichier .desktop Linux) —
        // pilotage côté webview via @tauri-apps/plugin-autostart. Sur macOS,
        // `LaunchAgent` (plutôt que le lancement caché natif de l'app elle-
        // même) évite de dupliquer l'entrée si l'app est aussi installée
        // via un vrai installeur.
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        // Sélecteur de fichiers natif : ouverture pour joindre un fichier par
        // chemin (`files.share`, jusqu'à 2 Gio) et sauvegarde pour copier un
        // blob complet au téléchargement (`files.save`). Pilotage côté webview
        // via @tauri-apps/plugin-dialog.
        .plugin(tauri_plugin_dialog::init())
        // Mise à jour intégrée (D-049) : le manifeste `latest.json` de la
        // dernière release GitHub est vérifié depuis la webview via
        // @tauri-apps/plugin-updater ; les artefacts sont authentifiés par
        // signature minisign (clé publique dans tauri.conf.json) avant
        // installation. `process` fournit le redémarrage post-installation.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(move |app| {
            // Répertoire de données par plateforme (ex. ~/Library/Application
            // Support/fr.accord.desktop) : racine du registre multi-comptes
            // (D-046) et du profil historique (`profil/`, jamais déplacé).
            // Charge/migre le registre puis active le compte le plus
            // récemment utilisé (voir `EtatHote::depuis_repertoire_app`).
            let app_data_dir = app.path().app_data_dir()?;
            // Le journal s'ouvre ici, dès que le dossier est connu : les
            // lignes déjà émises pendant l'amorçage y sont versées.
            journal.attacher_sous(&app_data_dir);
            tracing::info!(
                dossier = ?journal.dossier(),
                "journal de diagnostic ouvert"
            );
            app.manage(journal.clone());
            let etat = EtatHote::depuis_repertoire_app(app_data_dir)?;
            app.manage(etat);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commandes::vault_status,
            commandes::journal_ui,
            commandes::journal_dossier,
            commandes::journal_niveau,
            commandes::app_quit,
            commandes::create_identity,
            commandes::restore_identity,
            commandes::unlock,
            commandes::lock,
            commandes::accounts_list,
            commandes::account_create,
            commandes::account_restore,
            commandes::account_adopt_paired,
            commandes::account_unlock,
            commandes::session_close,
            commandes::backup_export,
            commandes::backup_import,
            commandes::ouvrir_reglages_systeme,
            commandes::micro_autorisation_etat,
            commandes::micro_autorisation_demander,
            tray::tray_set_enabled
        ])
        .build(tauri::generate_context!());

    match application {
        Ok(application) => {
            application.run(|app, evenement| {
                // Arrêt propre du nœud (réseau + API + base) à la fermeture.
                if let tauri::RunEvent::Exit = evenement {
                    if let Some(etat) = app.try_state::<EtatHote>() {
                        etat.arreter_noeud();
                    }
                }
            });
            ExitCode::SUCCESS
        }
        Err(e) => {
            tracing::error!(erreur = %e, "démarrage de l'hôte Tauri impossible");
            ExitCode::FAILURE
        }
    }
}
