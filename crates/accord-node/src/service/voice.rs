//! Méthodes `voice.*` (contrat gelé des salons vocaux D-025, périphériques
//! audio D-029, DSP de capture) et `calls.*` (appels 1-à-1, voir
//! docs/VOICE_CALLS.md).

use serde_json::{json, Value};

use crate::error::NodeError;
use crate::hex;
use crate::voice::{VoiceParticipant, VoiceRoomPresence, VoiceStatus};

use super::helpers::{param_device, param_id16, param_pubkey};
use super::NodeService;

/// Décode une chaîne hexadécimale de longueur variable (trame de partage
/// d'écran), bornée pour éviter une allocation démesurée depuis l'API locale —
/// une trame vidéo encodée reste très en dessous de la borne.
fn decode_hex_bounded(s: &str) -> Option<Vec<u8>> {
    /// Longueur hexadécimale maximale acceptée (1 MiB décodé).
    const MAX_HEX_LEN: usize = 2 * 1024 * 1024;
    if s.len() > MAX_HEX_LEN || s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Paramètres communs d'une trame vidéo (`screen.frame`, `camera.frame`) :
/// drapeau keyframe et octets encodés en hexadécimal.
fn video_frame_params(params: &Value) -> Result<(bool, Vec<u8>), NodeError> {
    let keyframe = params
        .get("keyframe")
        .and_then(Value::as_bool)
        .ok_or(NodeError::Invalid("keyframe booléen requis"))?;
    let data = params
        .get("data")
        .and_then(Value::as_str)
        .ok_or(NodeError::Invalid("data hexadécimal requis"))?;
    let encoded = decode_hex_bounded(data).ok_or(NodeError::Invalid(
        "data hexadécimal invalide ou trop grand",
    ))?;
    Ok((keyframe, encoded))
}

impl NodeService {
    /// Méthodes `voice.*` et `calls.*` (moteur voix requis).
    pub(super) async fn call_voice(
        &self,
        method: &str,
        params: &Value,
    ) -> Result<Value, NodeError> {
        let voice = self
            .voice
            .as_ref()
            .ok_or(NodeError::NotFound("sous-système voix indisponible"))?;
        match method {
            "calls.start" => {
                let peer = param_pubkey(params, "peer")?;
                let call_id = voice.call_start(peer).await?;
                Ok(json!({ "call_id": hex::encode(&call_id) }))
            }
            "calls.accept" => {
                let call_id = param_id16(params, "call_id")?;
                voice.call_accept(call_id).await?;
                Ok(json!({ "ok": true }))
            }
            "calls.decline" => {
                let call_id = param_id16(params, "call_id")?;
                voice.call_decline(call_id).await?;
                Ok(json!({ "ok": true }))
            }
            "calls.hangup" => {
                voice.call_hangup().await?;
                Ok(json!({ "ok": true }))
            }
            "calls.status" => {
                let snapshot = voice.call_status().await?;
                Ok(json!({
                    "state": snapshot.phase.as_str(),
                    "peer": snapshot.peer.map(|p| hex::encode(&p)),
                    "call_id": snapshot.call_id.map(|c| hex::encode(&c)),
                    "since_ms": snapshot.since_ms,
                }))
            }
            "voice.set_noise_suppression" => {
                let enabled = params
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .ok_or(NodeError::Invalid("enabled booléen requis"))?;
                voice.set_dsp(Some(enabled), None, None).await?;
                Ok(json!({}))
            }
            "voice.set_agc" => {
                let enabled = params
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .ok_or(NodeError::Invalid("enabled booléen requis"))?;
                voice.set_dsp(None, Some(enabled), None).await?;
                Ok(json!({}))
            }
            "voice.set_echo_cancel" => {
                let enabled = params
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .ok_or(NodeError::Invalid("enabled booléen requis"))?;
                voice.set_dsp(None, None, Some(enabled)).await?;
                Ok(json!({}))
            }
            "voice.join" => {
                let group_id = param_id16(params, "group_id")?;
                let channel_id = param_id16(params, "channel_id")?;
                let participants = voice.join(group_id, channel_id).await?;
                Ok(json!({
                    "participants": participants
                        .iter()
                        .map(|pk| hex::encode(pk))
                        .collect::<Vec<_>>(),
                }))
            }
            "voice.leave" => {
                voice.leave().await?;
                Ok(json!({}))
            }
            "voice.mute" => {
                let muted = params
                    .get("muted")
                    .and_then(Value::as_bool)
                    .ok_or(NodeError::Invalid("muted booléen requis"))?;
                voice.set_muted(muted).await?;
                Ok(json!({}))
            }
            "voice.deafen" => {
                let on = params
                    .get("on")
                    .and_then(Value::as_bool)
                    .ok_or(NodeError::Invalid("on booléen requis"))?;
                voice.set_deafened(on).await?;
                Ok(json!({}))
            }
            "voice.set_volume" => {
                // `peer` absent = master output volume.
                let peer = match params.get("peer") {
                    None | Some(Value::Null) => None,
                    Some(_) => Some(param_pubkey(params, "peer")?),
                };
                let volume = params
                    .get("volume")
                    .and_then(Value::as_u64)
                    .ok_or(NodeError::Invalid("volume entier requis"))?;
                let volume = u16::try_from(volume)
                    .map_err(|_| NodeError::Invalid("volume hors bornes (0 à 200)"))?;
                voice.set_volume(peer, volume).await?;
                Ok(json!({}))
            }
            "voice.status" => {
                let status = voice.status().await?;
                let master_volume = voice.master_volume().await?;
                let (noise_suppression, agc, echo_cancel) = voice.dsp_config().await?;
                Ok(json!({
                    "active": status.as_ref().map(voice_status_json),
                    "master_volume": master_volume,
                    "dsp": {
                        "noise_suppression": noise_suppression,
                        "agc": agc,
                        "echo_cancel": echo_cancel,
                    },
                }))
            }
            "voice.rooms" => {
                let rooms = voice.rooms().await?;
                Ok(json!({
                    "rooms": rooms.iter().map(voice_room_json).collect::<Vec<_>>(),
                }))
            }
            "voice.devices" => {
                let devices = voice.devices().await?;
                Ok(json!({
                    "inputs": devices.inputs,
                    "outputs": devices.outputs,
                    "selected_input": devices.selected_input,
                    "selected_output": devices.selected_output,
                }))
            }
            "voice.set_devices" => {
                let input = param_device(params, "input")?;
                let output = param_device(params, "output")?;
                voice.set_devices(input, output).await?;
                Ok(json!({}))
            }
            "voice.mic_test" => {
                let enabled = params
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .ok_or(NodeError::Invalid("enabled booléen requis"))?;
                voice.mic_test(enabled).await?;
                Ok(json!({}))
            }
            "screen.start" => {
                voice.screen_announce(true);
                Ok(json!({}))
            }
            "screen.stop" => {
                voice.screen_announce(false);
                Ok(json!({}))
            }
            "screen.frame" => {
                let (keyframe, encoded) = video_frame_params(params)?;
                voice.screen_send(keyframe, encoded);
                Ok(json!({}))
            }
            "camera.start" => {
                voice.camera_announce(true);
                Ok(json!({}))
            }
            "camera.stop" => {
                voice.camera_announce(false);
                Ok(json!({}))
            }
            "camera.frame" => {
                let (keyframe, encoded) = video_frame_params(params)?;
                voice.camera_send(keyframe, encoded);
                Ok(json!({}))
            }
            _ => Err(NodeError::Invalid("méthode inconnue")),
        }
    }
}

/// Rend l'état du salon vocal actif pour `voice.status` (contrat gelé,
/// étendu de façon additive : deafen, volumes, appels 1-à-1, modération
/// vocale et priorité d'orateur).
fn voice_status_json(status: &VoiceStatus) -> Value {
    json!({
        "group_id": hex::encode(&status.group_id),
        "channel_id": hex::encode(&status.channel_id),
        "is_call": status.is_call,
        "muted": status.muted,
        "deafened": status.deafened,
        "participants": status.participants.iter().map(voice_participant_json).collect::<Vec<_>>(),
    })
}

/// Rend un participant (forme partagée par `voice.status`, `voice.rooms`).
fn voice_participant_json(p: &VoiceParticipant) -> Value {
    json!({
        "pubkey": hex::encode(&p.pubkey),
        "speaking": p.speaking,
        "muted": p.muted,
        "deafened": p.deafened,
        "volume": p.volume,
        "server_muted": p.server_muted,
        "server_deafened": p.server_deafened,
        "priority_speaker": p.priority_speaker,
    })
}

/// Rend la présence d'un salon connu pour `voice.rooms` (occupants avec la
/// même forme de participant que `voice.status`).
fn voice_room_json(room: &VoiceRoomPresence) -> Value {
    json!({
        "group_id": hex::encode(&room.group_id),
        "channel_id": hex::encode(&room.channel_id),
        "participants": room.participants.iter().map(voice_participant_json).collect::<Vec<_>>(),
    })
}
