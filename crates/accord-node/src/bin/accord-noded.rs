//! Démon autonome `accord-noded` : exécute un nœud Accord et son API locale.
//!
//! Configuration par variables d'environnement (aucun secret en argument de
//! ligne de commande, visible dans `ps`) :
//! - `ACCORD_PROFILE` : répertoire de profil (défaut `./accord-profile`).
//! - `ACCORD_PASSPHRASE` : phrase de passe de l'identité (obligatoire).
//! - `ACCORD_API_PORT` : port de l'API locale (défaut éphémère).
//! - `ACCORD_P2P_ADDR` : adresse UDP d'écoute (défaut `0.0.0.0:0`).
//! - `ACCORD_POW_BITS` : difficulté PoW (défaut 16).
//!
//! Au démarrage, écrit `<profil>/session.json` (permissions 0600) contenant
//! l'adresse de l'API et le jeton d'authentification, que l'UI lit pour se
//! connecter.

use std::path::Path;
use std::process::ExitCode;

use accord_node::{identity, NodeConfig, Paths, RunningNode};

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(erreur = %e, "démarrage impossible");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let profile = std::env::var("ACCORD_PROFILE").unwrap_or_else(|_| "./accord-profile".into());
    let passphrase =
        std::env::var("ACCORD_PASSPHRASE").map_err(|_| "ACCORD_PASSPHRASE doit être définie")?;
    let api_port: u16 = parse_env("ACCORD_API_PORT", 0)?;
    let pow_bits: u32 = parse_env("ACCORD_POW_BITS", accord_proto::limits::IDENTITY_POW_BITS)?;
    let p2p_addr = std::env::var("ACCORD_P2P_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:0".into())
        .parse()?;

    let paths = Paths::new(&profile);
    let unlocked = if paths.has_identity() {
        tracing::info!("déverrouillage de l'identité existante");
        identity::unlock(&paths, &passphrase)?
    } else {
        tracing::info!(pow_bits, "création d'une nouvelle identité");
        identity::create(&paths, &passphrase, pow_bits)?
    };

    let config = NodeConfig {
        paths: paths.clone(),
        p2p_addr,
        api_port,
        pow_bits,
        // Rendez-vous partagé du premier contact (variable ACCORD_BOOTSTRAP).
        default_bootstrap: accord_node::default_bootstrap_env(),
        ..NodeConfig::default()
    };
    let running = accord_node::run(unlocked, config).await?;
    write_session(&paths.root, &running)?;

    tracing::info!(
        api = %running.api_addr(),
        p2p = %running.p2p_addr(),
        "nœud Accord démarré"
    );

    tokio::signal::ctrl_c().await?;
    tracing::info!("arrêt demandé");
    running.shutdown();
    Ok(())
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(v) => v.parse().map_err(|e| format!("{key} invalide : {e}")),
        Err(_) => Ok(default),
    }
}

/// Écrit le fichier de session (adresse API + jeton) en 0600.
fn write_session(root: &Path, running: &RunningNode) -> std::io::Result<()> {
    let session = format!(
        "{{\n  \"api\": \"{}\",\n  \"token\": \"{}\"\n}}\n",
        running.api_addr(),
        running.token.expose()
    );
    let path = root.join("session.json");
    std::fs::write(&path, session)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
