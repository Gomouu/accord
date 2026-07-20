//! Service API : traduit les méthodes JSON-RPC en opérations du [`Node`].
//!
//! Chaque méthode valide ses paramètres au niveau de la frontière (règle
//! « valider toute entrée externe ») puis délègue à la logique applicative.
//! Les identifiants transitent en hexadécimal ; aucune clé ni aucun corps de
//! message en clair n'est jamais journalisé.
//!
//! Le dispatch aiguille chaque famille de méthodes (préfixe) vers son
//! sous-module : [`dm`], [`groups`], [`friends`], [`voice`], [`profile`],
//! [`files`]. Les helpers de parsing partagés vivent dans [`helpers`].

use std::sync::Arc;

use accord_api::rpc::RpcError;
use accord_api::Service;
use accord_crypto::FriendCode;
use serde_json::Value;

use crate::error::NodeError;
use crate::node::Node;
use crate::voice::VoiceHandle;

mod dm;
mod files;
mod friends;
mod groups;
mod helpers;
mod mentions;
mod network;
mod profile;
mod voice;

#[cfg(test)]
mod tests;

/// Résolveur réseau de codes amis (record d'identité DHT vérifié). Implémenté
/// par le runtime réseau ; absent dans les tests sans réseau.
#[async_trait::async_trait]
pub trait CodeResolver: Send + Sync {
    /// Résout un code ami en clé publique vérifiée de bout en bout.
    async fn resolve(&self, code: &FriendCode) -> Result<[u8; 32], NodeError>;
}

/// Adaptateur API pour un nœud déverrouillé.
pub struct NodeService {
    node: Arc<Node>,
    resolver: Option<Arc<dyn CodeResolver>>,
    voice: Option<VoiceHandle>,
}

impl NodeService {
    /// Enveloppe un nœud sans résolution réseau (tests, outils locaux).
    pub fn new(node: Arc<Node>) -> Self {
        Self {
            node,
            resolver: None,
            voice: None,
        }
    }

    /// Enveloppe un nœud avec résolution de codes amis via la DHT.
    pub fn with_resolver(node: Arc<Node>, resolver: Arc<dyn CodeResolver>) -> Self {
        Self {
            node,
            resolver: Some(resolver),
            voice: None,
        }
    }

    /// Câble le sous-système voix (méthodes `voice.*`).
    #[must_use]
    pub fn with_voice(mut self, voice: VoiceHandle) -> Self {
        self.voice = Some(voice);
        self
    }
}

impl Service for NodeService {
    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        if method == "friends.resolve" {
            return self.resolve_code(&params).await.map_err(node_error_to_rpc);
        }
        if method.starts_with("voice.") || method.starts_with("calls.") {
            return self
                .call_voice(method, &params)
                .await
                .map_err(node_error_to_rpc);
        }
        if method.starts_with("network.") || method.starts_with("diagnostics.") {
            return self
                .call_network(method, &params)
                .await
                .map_err(node_error_to_rpc);
        }
        // Soundboard : ne diffuser un son que si l'appelant est connecté AU
        // salon vocal ciblé. Le statut du salon actif vit dans l'acteur voix,
        // interrogé ici (le service détient la poignée) et non dans
        // `Node::group_soundboard_play` (le `Node` n'a aucune poignée voix).
        // Un membre hors du vocal ne peut pas déclencher de son. Les
        // identifiants malformés retombent sur `dispatch`, qui rend l'erreur
        // de paramètre précise.
        if method == "groups.soundboard.play" {
            if let (Some(gid), Some(cid)) = (
                params
                    .get("group_id")
                    .and_then(Value::as_str)
                    .and_then(crate::hex::decode::<16>),
                params
                    .get("channel_id")
                    .and_then(Value::as_str)
                    .and_then(crate::hex::decode::<16>),
            ) {
                let connected = match &self.voice {
                    Some(voice) => voice
                        .status()
                        .await
                        .ok()
                        .flatten()
                        .is_some_and(|s| s.is_room(&gid, &cid)),
                    None => false,
                };
                if !connected {
                    return Err(node_error_to_rpc(NodeError::Invalid(
                        "pas connecté à ce salon vocal",
                    )));
                }
            }
        }
        let result = dispatch(&self.node, method, &params).map_err(node_error_to_rpc)?;
        // Une modération vocale émise localement doit s'appliquer sans
        // attendre : le moteur voix rafraîchit son cache immédiatement.
        if method == "groups.voice_moderate" {
            if let Some(voice) = &self.voice {
                if let Some(group_id) = params
                    .get("group_id")
                    .and_then(Value::as_str)
                    .and_then(crate::hex::decode::<16>)
                {
                    voice.group_changed(group_id);
                }
            }
        }
        Ok(result)
    }
}

/// Convertit une erreur applicative en erreur JSON-RPC (messages génériques,
/// sans fuite d'information sensible).
fn node_error_to_rpc(e: NodeError) -> RpcError {
    match e {
        NodeError::Invalid(m) => RpcError::invalid_params(m),
        NodeError::NotFound(m) => RpcError::app(format!("introuvable : {m}")),
        NodeError::Core(accord_core::CoreError::OpRejected(m)) => {
            RpcError::app(format!("refusé : {m}"))
        }
        NodeError::Core(accord_core::CoreError::Invalid(m)) => RpcError::invalid_params(m),
        NodeError::Core(accord_core::CoreError::NotFound(m)) => {
            RpcError::app(format!("introuvable : {m}"))
        }
        other => RpcError::app(other.to_string()),
    }
}

/// Méthodes synchrones : délègue au sous-module du préfixe de la méthode.
fn dispatch(node: &Node, method: &str, params: &Value) -> Result<Value, NodeError> {
    if method == "identity.self" || method.starts_with("profile.") {
        return profile::dispatch(node, method, params);
    }
    if method.starts_with("friends.") || method == "search.query" {
        return friends::dispatch(node, method, params);
    }
    if method.starts_with("dm.") {
        return dm::dispatch(node, method, params);
    }
    if method.starts_with("groups.") {
        return groups::dispatch(node, method, params);
    }
    if method.starts_with("mentions.") {
        return mentions::dispatch(node, method, params);
    }
    if method.starts_with("files.") {
        return files::dispatch(node, method, params);
    }
    Err(NodeError::Invalid("méthode inconnue"))
}
