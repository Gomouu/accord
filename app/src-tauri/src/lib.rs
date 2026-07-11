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

use std::process::ExitCode;

use tauri::Manager;

use etat::EtatHote;

/// Construit puis lance l'application Tauri. Rend un code de sortie explicite
/// au lieu de paniquer en cas d'échec de démarrage.
pub fn executer() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let application = tauri::Builder::default()
        // Notifications système natives (D-028) : l'envoi se fait côté
        // webview via @tauri-apps/plugin-notification.
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // Répertoire de données par plateforme (ex. ~/Library/Application
            // Support/fr.accord.desktop) : racine du registre multi-comptes
            // (D-046) et du profil historique (`profil/`, jamais déplacé).
            // Charge/migre le registre puis active le compte le plus
            // récemment utilisé (voir `EtatHote::depuis_repertoire_app`).
            let app_data_dir = app.path().app_data_dir()?;
            let etat = EtatHote::depuis_repertoire_app(app_data_dir)?;
            app.manage(etat);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commandes::vault_status,
            commandes::create_identity,
            commandes::restore_identity,
            commandes::unlock,
            commandes::lock,
            commandes::accounts_list,
            commandes::account_create,
            commandes::account_restore,
            commandes::account_unlock,
            commandes::session_close
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
