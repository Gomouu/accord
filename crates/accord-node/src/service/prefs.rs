//! `prefs.*` methods (lot 7.2): the account-level preferences the UI mirrors
//! to the node so they follow the person across devices.
//!
//! The UI keeps `localStorage` as its local source of truth — it hydrates
//! synchronously at store construction while the node is async — and mirrors
//! here. `prefs.list` is what it reads at startup to find out whether another
//! device changed something more recently.

use serde_json::{json, Value};

use crate::error::NodeError;
use crate::node::Node;

use super::helpers::param_str;

/// Routes the `prefs.*` methods to the node.
pub(super) fn dispatch(node: &Node, method: &str, params: &Value) -> Result<Value, NodeError> {
    match method {
        "prefs.list" => {
            let prefs: Vec<Value> = node
                .synced_prefs()?
                .into_iter()
                .map(|p| json!({ "key": p.key, "value": p.value, "at_ms": p.at_ms }))
                .collect();
            Ok(json!({ "prefs": prefs }))
        }
        "prefs.set" => {
            // A key outside the allowlist is an error here, not a silent
            // ignore: this caller is our own UI, and a preference it believes
            // is synced while it is not would otherwise surface months later
            // as "this setting does not follow me".
            let key = param_str(params, "key")?;
            let value = param_str(params, "value")?;
            Ok(json!({ "at_ms": node.set_synced_pref(key, value)? }))
        }
        _ => Err(NodeError::Invalid("méthode inconnue")),
    }
}
