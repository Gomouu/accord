//! Nœud de rendez-vous headless `accord-bootstrap` : routage DHT + service de
//! relais, SANS compte utilisateur ni interaction.
//!
//! Un rendez-vous n'a besoin que d'une identité de NŒUD (paire de clés + PoW,
//! exigée par le handshake transport et le `NodeInfo` DHT) — pas d'un compte
//! opéré par un humain : la phrase de passe qui scelle cette identité est donc
//! générée aléatoirement au premier lancement et persistée dans l'état local
//! (fichier 0600), pour conserver un `node_id` stable d'un redémarrage à
//! l'autre. Le service de relais et le drapeau RELAY sont câblés d'office par
//! `accord_node::run` (SPEC §10-§11.3) ; l'API locale reste liée à 127.0.0.1
//! et aucun `session.json` n'est écrit (aucune UI ne s'y connecte).
//!
//! Configuration par variables d'environnement :
//! - `ACCORD_BOOTSTRAP_STATE` : répertoire d'état (défaut `./accord-bootstrap-state`).
//! - `ACCORD_BOOTSTRAP_P2P_ADDR` : adresse UDP d'écoute (défaut `0.0.0.0:48016`).
//! - `ACCORD_BOOTSTRAP_POW_BITS` : difficulté PoW exigée des pairs (défaut protocole).
//! - `ACCORD_BOOTSTRAP_PASSPHRASE` : phrase de passe explicite (sinon générée).
//! - `ACCORD_BOOTSTRAP_NAT` : `1` active le mapping de port UPnP/NAT-PMP (défaut `0`,
//!   un rendez-vous est censé être publiquement joignable).
//! - `ACCORD_BOOTSTRAP_MDNS` : `1` active l'annonce LAN (défaut `0`).
//! - `ACCORD_BOOTSTRAP` : autres rendez-vous à mailler, `"ip:port,ip:port"` —
//!   INDISPENSABLE dès 2 hôtes : le consensus des `ObservedAddrs` (M1b) exige
//!   2 observateurs distincts, et un maillage connexe l'assure.

use std::path::Path;
use std::process::ExitCode;

use accord_node::{identity, NodeConfig, NodeError, Paths, VoiceBackend};

/// Erreurs de démarrage propres au rendez-vous.
#[derive(Debug, thiserror::Error)]
enum BootstrapError {
    /// Variable d'environnement mal formée.
    #[error("{0}")]
    Config(String),
    /// Échec nœud (identité, base, réseau).
    #[error(transparent)]
    Node(#[from] NodeError),
    /// E/S sur l'état local (phrase de passe).
    #[error("état local : {0}")]
    Io(#[from] std::io::Error),
}

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

async fn run() -> Result<(), BootstrapError> {
    let state = std::env::var("ACCORD_BOOTSTRAP_STATE")
        .unwrap_or_else(|_| "./accord-bootstrap-state".into());
    let p2p_addr = std::env::var("ACCORD_BOOTSTRAP_P2P_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:48016".into())
        .parse()
        .map_err(|e| BootstrapError::Config(format!("ACCORD_BOOTSTRAP_P2P_ADDR invalide : {e}")))?;
    let pow_bits: u32 = parse_env(
        "ACCORD_BOOTSTRAP_POW_BITS",
        accord_proto::limits::IDENTITY_POW_BITS,
    )?;
    let nat_enabled = flag_env("ACCORD_BOOTSTRAP_NAT", false);
    let mdns_enabled = flag_env("ACCORD_BOOTSTRAP_MDNS", false);

    std::fs::create_dir_all(&state)?;
    let paths = Paths::new(&state);
    let passphrase = load_or_create_passphrase(Path::new(&state))?;

    let unlocked = if paths.has_identity() {
        tracing::info!("déverrouillage de l'identité de nœud existante");
        identity::unlock(&paths, &passphrase)?
    } else {
        tracing::info!(pow_bits, "création de l'identité de nœud (sans compte)");
        identity::create(&paths, &passphrase, pow_bits)?
    };

    let config = NodeConfig {
        paths,
        p2p_addr,
        api_port: 0,
        pow_bits,
        // Aucun périphérique audio sur un hôte headless : backend simulé.
        voice_backend: VoiceBackend::Simule,
        nat_enabled,
        mdns_enabled,
        // Maillage avec les autres rendez-vous (variable ACCORD_BOOTSTRAP).
        default_bootstrap: accord_node::default_bootstrap_env(),
    };
    let running = accord_node::run(unlocked, config).await?;

    tracing::info!(
        p2p = %running.p2p_addr(),
        node_id = %accord_node::hex::encode(&running.node_info().node_id.0),
        rendez_vous = ?running.network_status().bootstrap,
        "nœud de rendez-vous démarré (DHT + relais, sans compte)"
    );

    wait_for_shutdown().await?;
    tracing::info!("arrêt demandé");
    running.shutdown();
    Ok(())
}

/// Attend Ctrl-C ou, sous Unix, SIGTERM (arrêt propre demandé par systemd ou
/// par `docker stop` — sans quoi le conteneur est tué après le délai de grâce).
async fn wait_for_shutdown() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            r = tokio::signal::ctrl_c() => r,
            _ = sigterm.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await
    }
}

/// Phrase de passe machine : `ACCORD_BOOTSTRAP_PASSPHRASE` si fournie, sinon
/// lue depuis `<état>/passphrase`, sinon générée (32 octets d'OsRng, hex) et
/// persistée en 0600. Elle ne protège qu'une identité de nœud sans données
/// utilisateur — sa perte se répare en repartant d'un état vierge (nouveau
/// `node_id`), au prix d'une mise à jour d'`ACCORD_BOOTSTRAP` nulle part
/// nécessaire : les pairs adressent le rendez-vous par `ip:port`.
fn load_or_create_passphrase(state: &Path) -> Result<String, BootstrapError> {
    if let Ok(pass) = std::env::var("ACCORD_BOOTSTRAP_PASSPHRASE") {
        if !pass.is_empty() {
            return Ok(pass);
        }
    }
    let path = state.join("passphrase");
    if path.exists() {
        return Ok(std::fs::read_to_string(&path)?.trim().to_string());
    }
    let mut bytes = [0u8; 32];
    use rand::RngCore;
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let pass = accord_node::hex::encode(&bytes);
    std::fs::write(&path, &pass)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(pass)
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> Result<T, BootstrapError>
where
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(v) => v
            .parse()
            .map_err(|e| BootstrapError::Config(format!("{key} invalide : {e}"))),
        Err(_) => Ok(default),
    }
}

/// Drapeau d'environnement : `1`/`true` vrai, `0`/`false` faux, défaut sinon.
fn flag_env(key: &str, default: bool) -> bool {
    match std::env::var(key).ok().as_deref() {
        Some("1") | Some("true") => true,
        Some("0") | Some("false") => false,
        _ => default,
    }
}
