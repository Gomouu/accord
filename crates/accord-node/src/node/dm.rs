//! Messagerie directe : composition et routage des messages, éditions,
//! suppressions et réactions (bloc `impl Node` du domaine `dm.*`).

use std::collections::{BTreeSet, HashMap, HashSet};

use accord_core::db::DmRecord;
use accord_core::messaging;
use accord_proto::core_msg::{CoreMsg, FileRef, SELF_READ_SCOPE_DM};
use serde_json::{json, Value};

use crate::error::NodeError;
use crate::hex;
use crate::outbound::Outbound;

use super::{dm_mark_key, now_ms, read_u64, Node};

/// Meta key of the read-receipts privacy toggle (absent = enabled).
const READ_RECEIPTS_KEY: &str = "dm.read_receipts";

/// Direct-send attempts after which an unacked message is surfaced as
/// `failed`. The message keeps being retried in the background (backoff) until
/// the offline-queue expiry; `failed` is a UI hint, not a terminal state.
const DM_FAILED_ATTEMPTS: u32 = 5;

/// Offline-queue lifetime (SPEC §7, mirrors `db::outbox::QUEUE_EXPIRY_MS`):
/// past it, an unacked message that is no longer queued is `failed`.
const DM_QUEUE_EXPIRY_MS: u64 = 7 * 24 * 3600 * 1000;

/// Per-message delivery state derived from the ack flag and the outbox.
///
/// - `sent`: acked by the peer (or an incoming message, delivered by definition);
/// - `failed`: our unacked message whose direct retries are exhausted
///   (`attempts >= DM_FAILED_ATTEMPTS`) or which is unacked, no longer queued
///   and older than the queue expiry;
/// - `pending`: our unacked message still in flight or being retried.
///
/// 🔒 `synced` porte les messages **rattrapés** depuis un autre de nos
/// appareils. Ils sont de nous, donc `author == me`, mais cette machine ne les
/// a jamais composés : elle n'a aucune ligne d'outbox pour eux, n'en aura
/// jamais, et n'a donc rien à dire de leur livraison. Sans cette distinction,
/// la seule absence de ligne suffirait à les déclarer **échoués** passé la
/// rétention de la file — un message parfaitement livré depuis le bureau
/// s'afficherait en rouge sur le portable une semaine plus tard, et c'est la
/// première chose que verrait l'utilisateur sur la machine qui a rattrapé.
fn dm_delivery_state(
    rec: &DmRecord,
    me: &[u8; 32],
    outbox: &HashMap<[u8; 16], (u32, bool)>,
    synced: &HashSet<[u8; 16]>,
    now: u64,
) -> &'static str {
    if rec.acked || rec.author != *me {
        return "sent";
    }
    match outbox.get(&rec.msg_id) {
        Some((attempts, _)) if *attempts >= DM_FAILED_ATTEMPTS => "failed",
        Some(_) => "pending",
        // Rattrapé et non acquitté : l'appareil qui l'a composé savait, au
        // moment de nous le passer, qu'il n'était pas encore livré. C'est tout
        // ce que nous saurons jamais — d'où « en cours », qui ne promet rien,
        // plutôt qu'« échec », qui accuse à tort.
        None if synced.contains(&rec.msg_id) => "pending",
        None if now.saturating_sub(rec.sent_ms) > DM_QUEUE_EXPIRY_MS => "failed",
        None => "pending",
    }
}

/// Rend une liste de pièces jointes en JSON (forme gelée côté UI).
pub(super) fn attachments_json(attachments: &[FileRef]) -> Value {
    Value::Array(
        attachments
            .iter()
            .map(|a| {
                json!({
                    "merkle_root": hex::encode(&a.merkle_root),
                    "name": a.name,
                    "size": a.size,
                    "mime": a.mime,
                })
            })
            .collect(),
    )
}

impl Node {
    /// Compose et route un message texte ; persiste et met en file si le pair
    /// est hors ligne (géré par la boucle réseau).
    pub fn dm_send(
        &self,
        peer_pubkey: &[u8; 32],
        text: &str,
        reply_to: Option<[u8; 16]>,
    ) -> Result<String, NodeError> {
        self.dm_send_with_attachments(peer_pubkey, text, reply_to, vec![])
    }

    /// Compose et route un message texte avec pièces jointes (≤ 10, déjà
    /// publiées dans le magasin de fichiers).
    pub fn dm_send_with_attachments(
        &self,
        peer_pubkey: &[u8; 32],
        text: &str,
        reply_to: Option<[u8; 16]>,
        attachments: Vec<FileRef>,
    ) -> Result<String, NodeError> {
        let msg = self.with_db(|db| {
            Ok(messaging::compose_text(
                db,
                &self.identity,
                &self.search_key,
                peer_pubkey,
                text,
                reply_to,
                attachments,
                now_ms(),
            )?)
        })?;
        let msg_id = match &msg {
            CoreMsg::DirectMsg { msg_id, .. } => hex::encode(msg_id),
            _ => unreachable!("compose_text produit un DirectMsg"),
        };
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        Ok(msg_id)
    }

    /// Pièces jointes persistées d'un message (DM ou groupe).
    pub fn attachments_of(&self, msg_id: &[u8; 16]) -> Result<Vec<FileRef>, NodeError> {
        self.with_db(|db| Ok(db.msg_attachments(msg_id)?))
    }

    /// Historique d'une conversation directe.
    pub fn dm_history(
        &self,
        peer_pubkey: &[u8; 32],
        before_lamport: u64,
        limit: usize,
    ) -> Result<Vec<DmRecord>, NodeError> {
        self.with_db(|db| Ok(db.dm_history(peer_pubkey, before_lamport, limit)?))
    }

    /// Émet un indicateur de frappe éphémère vers un ami. Jamais persisté,
    /// jamais mis en file : si le pair n'est pas présumé en ligne, silence
    /// (« pair injoignable = silencieusement ignoré », SPEC §6).
    pub fn dm_typing(&self, peer_pubkey: &[u8; 32]) -> Result<(), NodeError> {
        if !self.is_online(peer_pubkey) {
            return Ok(());
        }
        let msg = self.with_db(|db| {
            Ok(messaging::compose_typing(
                db,
                &self.identity,
                peer_pubkey,
                now_ms(),
            )?)
        })?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        Ok(())
    }

    /// Marque la conversation avec `peer` lue jusqu'à `lamport` (position
    /// locale, persistée dans les métadonnées, pour le calcul des non-lus).
    ///
    /// La marque est monotone : une position inférieure ou égale à celle déjà
    /// retenue est ignorée en silence.
    ///
    /// Best-effort read receipt (ephemeral, like typing): when the mark
    /// actually advances, the privacy toggle is on and the peer is presumed
    /// online, a `ReadReceipt` targeting the peer's latest covered message is
    /// emitted — never persisted, never queued offline.
    ///
    /// Une marque qui avance est par ailleurs répercutée à nos AUTRES
    /// appareils ([`Node::sync_read_mark_to_own_devices`]) — deux
    /// destinataires distincts, deux messages distincts, et surtout deux
    /// règles distinctes : voir le 🔒 posé sur la seconde.
    pub fn dm_mark_read(&self, peer_pubkey: &[u8; 32], lamport: u64) -> Result<(), NodeError> {
        let key = dm_mark_key(peer_pubkey);
        let previous = self.with_db(|db| Ok(read_u64(db.meta(&key)?)))?;
        // Un compte a plusieurs appareils, et « lu = lu sur au moins un
        // appareil » (MULTI_DEVICE.md §5). Sans cette monotonie, un appareil
        // resté trois jours éteint rouvrirait la conversation sur sa vieille
        // position et ferait « redevenir non lus » des messages lus depuis
        // longtemps : la conversation se remarquerait non lue toute seule ici,
        // et l'accusé émis rembobinerait la vue de l'expéditeur. Un appareil en
        // retard n'est pas une erreur — sa position est ignorée en silence.
        let advanced = lamport > previous;
        if advanced {
            self.with_db(|db| Ok(db.set_meta(&key, &lamport.to_be_bytes())?))?;
            self.sync_read_mark_to_own_devices(peer_pubkey, lamport)?;
        }
        // Ouvrir une conversation éteint ses mentions non lues (parité Discord).
        // Sans condition de position : acquitter une mention ne va que dans le
        // sens du lu, donc ne peut pas faire régresser l'état.
        self.with_db(|db| Ok(db.mark_dm_mentions_read(peer_pubkey)?))?;
        // Throttle: only marks that advance emit a receipt (re-marking the
        // same position, e.g. on window refocus, stays silent).
        if !advanced || !self.read_receipts_enabled()? || !self.is_online(peer_pubkey) {
            return Ok(());
        }
        let receipt = self.with_db(|db| {
            let Some(up_to) = db.latest_dm_from_peer(peer_pubkey, lamport)? else {
                return Ok(None);
            };
            Ok(Some(messaging::compose_read_receipt(
                db,
                &self.identity,
                peer_pubkey,
                &up_to,
                now_ms(),
            )?))
        });
        // Best-effort: a receipt that cannot be composed (e.g. the contact
        // is not a friend anymore) is silently dropped.
        if let Ok(Some(msg)) = receipt {
            self.outbound.send(Outbound::Core {
                to: *peer_pubkey,
                msg: Box::new(msg),
            });
        }
        Ok(())
    }

    /// Répercute notre position de lecture aux AUTRES appareils du compte
    /// (« lu = lu sur au moins un appareil », `docs/MULTI_DEVICE.md` §5).
    ///
    /// 🔒 **Jamais conditionnée au réglage des accusés de lecture.** Ce
    /// réglage décide de ce que le PAIR apprend ; celui-ci ne quitte pas le
    /// compte. Les confondre ferait cesser la synchronisation entre les
    /// machines de l'utilisateur à l'instant où il coupe un réglage qui n'en
    /// parle pas — une option de confidentialité qui casse en douce une
    /// fonction sans rapport est le pire genre de surprise.
    ///
    /// Ni conditionnée à la présence en ligne : la cible est notre propre
    /// compte, pas le pair, et c'est la couche de livraison qui sait quels
    /// appareils sont joignables.
    ///
    /// ⚠️ La marque est **éphémère** : `maintenance::is_queueable_offline` ne
    /// la met pas en file, donc un appareil éteint ne la rattrape pas. Coût
    /// assumé — il se remet à jour tout seul dès qu'on ouvre la conversation
    /// chez lui, et une file de marques périmées ne ferait que rejouer des
    /// positions déjà dépassées.
    fn sync_read_mark_to_own_devices(
        &self,
        peer_pubkey: &[u8; 32],
        lamport: u64,
    ) -> Result<(), NodeError> {
        // Ramené à un message DU PAIR : c'est cette normalisation qui rend la
        // marque transposable d'une machine à l'autre (voir le ⚠️ sur
        // `CoreMsg::SelfReadMark::up_to`). Rien à annoncer quand aucun message
        // du pair n'est couvert — le compteur de non-lus des autres appareils
        // ne se calcule que sur ceux-là, il ne bougerait pas d'un pouce.
        let Some(up_to) = self.with_db(|db| Ok(db.latest_dm_from_peer(peer_pubkey, lamport)?))?
        else {
            return Ok(());
        };
        self.outbound.send(Outbound::Core {
            // Adressé au COMPTE : la couche réseau le développe en un envoi
            // par appareil joignable. Nous y figurons peut-être nous-mêmes,
            // sans conséquence — l'ingestion est un `max`, donc idempotente.
            to: self.public_key(),
            msg: Box::new(CoreMsg::SelfReadMark {
                scope: SELF_READ_SCOPE_DM,
                conv: *peer_pubkey,
                up_to,
            }),
        });
        Ok(())
    }

    /// Ingère une marque de lecture émise par un autre appareil de NOTRE
    /// compte : la position locale avance jusqu'au message désigné.
    ///
    /// Silencieuse dans tous les cas de refus : un message qui n'est pas des
    /// nôtres, une portée qu'on ne sait pas traiter ou un identifiant inconnu
    /// ne sont pas des erreurs à remonter, seulement des marques sans effet.
    pub(super) fn ingest_self_read_mark(
        &self,
        sender: &[u8; 32],
        scope: u8,
        conv: &[u8; 32],
        up_to: &[u8; 16],
    ) -> Result<(), NodeError> {
        // 🔒 Le seul contrôle d'autorisation du chemin : sans lui, n'importe
        // quel ami pourrait éteindre le badge de n'importe quelle
        // conversation. Le décodage a déjà écarté les portées inconnues.
        if scope != SELF_READ_SCOPE_DM || !self.is_own_device(sender) {
            return Ok(());
        }
        // La position vient de NOTRE base : un identifiant que cet appareil ne
        // connaît pas ne couvre rien ici. C'est ce qui empêche une marque
        // émise ailleurs de faire passer pour lus des messages du pair que
        // cette machine n'a jamais reçus.
        let Some(lamport) = self.with_db(|db| Ok(db.dm_lamport(up_to)?))? else {
            return Ok(());
        };
        let key = dm_mark_key(conv);
        // Même monotonie que le marquage local : une marque partie d'un
        // appareil en retard ne doit pas faire redevenir non lus des messages
        // lus depuis longtemps sur celui-ci.
        if lamport <= self.with_db(|db| Ok(read_u64(db.meta(&key)?)))? {
            return Ok(());
        }
        self.with_db(|db| Ok(db.set_meta(&key, &lamport.to_be_bytes())?))?;
        self.emit(
            "event.dm_self_read",
            json!({ "peer": hex::encode(conv), "lamport": lamport }),
        );
        Ok(())
    }

    /// Vrai si `key` est une clé sous laquelle un appareil de NOTRE compte se
    /// présente au transport.
    ///
    /// 🔒 Deux formes acceptées, parce que le parc est à cheval sur les deux
    /// phases du basculement (`docs/MULTI_DEVICE.md` §3.2.1) :
    ///
    /// - la clé de **compte**, que présentent encore tous les appareils non
    ///   basculés. Nul ne peut la présenter au transport sans détenir la
    ///   graine du compte — donc sans être une de nos machines ;
    /// - la clé d'**appareil** une fois le transport basculé, qui doit alors
    ///   figurer dans notre propre liste signée.
    ///
    /// Ne garder que la seconde couperait une direction sur deux du parc
    /// mixte : un appareil non basculé parlant à un appareil basculé serait
    /// refusé ici, et lui seul — panne asymétrique, donc invisible aux tests
    /// qui n'essaient qu'un sens.
    fn is_own_device(&self, key: &[u8; 32]) -> bool {
        if *key == self.public_key() {
            return true;
        }
        // Pas de liste lisible = rien qui prouve quoi que ce soit : on refuse,
        // comme `DeviceList::authorises` refuse une liste incohérente.
        self.current_device_list()
            .map(|list| list.authorises(key))
            .unwrap_or(false)
    }

    /// Vrai si l'émission des accusés de lecture est activée (réglage de
    /// confidentialité, persisté dans la table meta ; activé par défaut).
    /// Les accusés entrants restent enregistrés quel que soit le réglage.
    pub fn read_receipts_enabled(&self) -> Result<bool, NodeError> {
        self.with_db(|db| {
            Ok(db
                .meta(READ_RECEIPTS_KEY)?
                .map(|v| v.first() != Some(&0))
                .unwrap_or(true))
        })
    }

    /// Active ou coupe l'émission des accusés de lecture (persisté).
    pub fn set_read_receipts(&self, enabled: bool) -> Result<(), NodeError> {
        self.with_db(|db| Ok(db.set_meta(READ_RECEIPTS_KEY, &[u8::from(enabled)])?))
    }

    /// Position de lecture du pair dans la conversation (lamport du dernier
    /// message couvert par son accusé de lecture), si connue.
    pub fn dm_peer_read_lamport(&self, peer_pubkey: &[u8; 32]) -> Result<Option<u64>, NodeError> {
        self.with_db(|db| {
            let Some(msg_id) = db.read_mark(peer_pubkey)? else {
                return Ok(None);
            };
            Ok(db.dm_lamport(&msg_id)?)
        })
    }

    /// Nombre de messages du pair reçus après notre marque de lecture locale.
    pub fn dm_unread(&self, peer_pubkey: &[u8; 32]) -> Result<u64, NodeError> {
        self.with_db(|db| {
            let mark = read_u64(db.meta(&dm_mark_key(peer_pubkey))?);
            Ok(db.count_dm_unread(peer_pubkey, mark)?)
        })
    }

    /// Notre propre marque de lecture locale dans cette conversation (lamport
    /// du dernier message lu, `0` si jamais marquée) — sert au séparateur
    /// « nouveaux messages » de l'UI, capturé à l'ouverture avant le marquage.
    pub fn dm_read_lamport(&self, peer_pubkey: &[u8; 32]) -> Result<u64, NodeError> {
        self.with_db(|db| Ok(read_u64(db.meta(&dm_mark_key(peer_pubkey))?)))
    }

    /// Édite un de nos messages directs (auteur seul, refusé sinon) puis
    /// route l'édition vers le pair, sur le même chemin que [`Node::dm_send`].
    pub fn dm_edit(
        &self,
        peer_pubkey: &[u8; 32],
        target: &[u8; 16],
        new_text: &str,
    ) -> Result<(), NodeError> {
        let msg = self.with_db(|db| {
            Ok(messaging::compose_edit(
                db,
                &self.identity,
                &self.search_key,
                peer_pubkey,
                target,
                new_text,
                now_ms(),
            )?)
        })?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        Ok(())
    }

    /// Supprime un de nos messages directs (tombstone local immédiat) puis
    /// route la suppression vers le pair.
    pub fn dm_delete(&self, peer_pubkey: &[u8; 32], target: &[u8; 16]) -> Result<(), NodeError> {
        let msg = self.with_db(|db| {
            Ok(messaging::compose_delete(
                db,
                &self.identity,
                peer_pubkey,
                target,
                now_ms(),
            )?)
        })?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        Ok(())
    }

    /// Ajoute (`add = true`) ou retire une réaction sur un message direct,
    /// applique le changement localement puis le route vers le pair.
    pub fn dm_react(
        &self,
        peer_pubkey: &[u8; 32],
        target: &[u8; 16],
        emoji: &str,
        add: bool,
    ) -> Result<(), NodeError> {
        let msg = self.with_db(|db| {
            Ok(messaging::compose_reaction(
                db,
                &self.identity,
                peer_pubkey,
                target,
                emoji,
                add,
                now_ms(),
            )?)
        })?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        Ok(())
    }

    /// Fenêtre d'historique centrée sur `msg_id` (jump-to-message) : moitié
    /// avant, la cible, moitié après. Rend `(fenêtre, found)` ; `found = false`
    /// avec une fenêtre vide si la cible est inconnue localement.
    pub fn dm_history_around(
        &self,
        peer_pubkey: &[u8; 32],
        msg_id: &[u8; 16],
        limit: usize,
    ) -> Result<(Vec<DmRecord>, bool), NodeError> {
        self.with_db(
            |db| match db.dm_history_around(peer_pubkey, msg_id, limit)? {
                Some(window) => Ok((window, true)),
                None => Ok((Vec::new(), false)),
            },
        )
    }

    /// Épingle un message direct puis réplique l'état au pair sur le chemin
    /// fiable de [`Node::dm_send`]. Le message doit être connu localement et
    /// appartenir à cette conversation ; le jeu d'épingles est borné à
    /// [`messaging::MAX_DM_PINS`].
    pub fn dm_pin(&self, peer_pubkey: &[u8; 32], msg_id: &[u8; 16]) -> Result<(), NodeError> {
        let msg = self.with_db(|db| {
            match db.dm_message(msg_id)? {
                Some(rec) if rec.peer == *peer_pubkey => {}
                _ => {
                    return Err(NodeError::NotFound(
                        "message inconnu dans cette conversation",
                    ))
                }
            }
            let pins = db.dm_pins(peer_pubkey)?;
            if pins.len() >= messaging::MAX_DM_PINS && !pins.contains(msg_id) {
                return Err(NodeError::Invalid("trop de messages épinglés"));
            }
            db.dm_pin(peer_pubkey, msg_id)?;
            Ok(messaging::compose_pin(
                db,
                &self.identity,
                peer_pubkey,
                msg_id,
                true,
                now_ms(),
            )?)
        })?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        Ok(())
    }

    /// Retire l'épingle d'un message direct (sans effet si absente) puis
    /// réplique le retrait au pair sur le chemin fiable de [`Node::dm_send`].
    pub fn dm_unpin(&self, peer_pubkey: &[u8; 32], msg_id: &[u8; 16]) -> Result<(), NodeError> {
        let msg = self.with_db(|db| {
            db.dm_unpin(peer_pubkey, msg_id)?;
            Ok(messaging::compose_pin(
                db,
                &self.identity,
                peer_pubkey,
                msg_id,
                false,
                now_ms(),
            )?)
        })?;
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        Ok(())
    }

    /// Messages épinglés d'une conversation directe (hex), ordre d'identifiant.
    pub fn dm_pins(&self, peer_pubkey: &[u8; 32]) -> Result<Vec<String>, NodeError> {
        self.with_db(|db| {
            Ok(db
                .dm_pins(peer_pubkey)?
                .iter()
                .map(|id| hex::encode(id))
                .collect())
        })
    }

    /// Ensemble des messages épinglés (annotation `pinned` de l'historique).
    pub fn dm_pinned_set(&self, peer_pubkey: &[u8; 32]) -> Result<BTreeSet<[u8; 16]>, NodeError> {
        self.with_db(|db| Ok(db.dm_pinned_set(peer_pubkey)?))
    }

    /// État de livraison des `DirectMsg` encore en file pour un pair
    /// (`msg_id → (tentatives, déposé en boîte)`), pour calculer `delivery`.
    pub fn dm_outbox_states(
        &self,
        peer_pubkey: &[u8; 32],
    ) -> Result<HashMap<[u8; 16], (u32, bool)>, NodeError> {
        self.with_db(|db| {
            let mut map = HashMap::new();
            for item in db.outbox_for(peer_pubkey)? {
                if let Ok(CoreMsg::DirectMsg { msg_id, .. }) =
                    crate::maintenance::decode_core(&item.payload)
                {
                    map.insert(msg_id, (item.attempts, item.mailboxed_day > 0));
                }
            }
            Ok(map)
        })
    }

    /// Messages d'une conversation obtenus par rattrapage depuis un autre de
    /// nos appareils (annotation locale, cf. [`Node::dm_delivery`]).
    pub fn dm_synced_states(&self, peer_pubkey: &[u8; 32]) -> Result<HashSet<[u8; 16]>, NodeError> {
        self.with_db(|db| Ok(db.dm_synced_set(peer_pubkey)?))
    }

    /// État de livraison d'un message (`"sent"` | `"pending"` | `"failed"`),
    /// dérivé de l'accusé, de la file d'attente (`dm_outbox_states`) et de
    /// l'origine locale de la ligne (`dm_synced_states`).
    pub fn dm_delivery(
        &self,
        rec: &DmRecord,
        outbox: &HashMap<[u8; 16], (u32, bool)>,
        synced: &HashSet<[u8; 16]>,
    ) -> &'static str {
        dm_delivery_state(rec, &self.public_key(), outbox, synced, now_ms())
    }

    /// Relance l'envoi d'un de nos messages directs non acquitté (jump-to-retry
    /// d'un état `failed`/`pending`). Purge toute copie en file (backoff remis à
    /// zéro) puis réémet sur le même chemin que [`Node::dm_send`].
    pub fn dm_retry(&self, peer_pubkey: &[u8; 32], msg_id: &[u8; 16]) -> Result<(), NodeError> {
        let me = self.public_key();
        let rec = self
            .with_db(|db| Ok(db.dm_message(msg_id)?))?
            .ok_or(NodeError::NotFound("message inconnu"))?;
        if rec.peer != *peer_pubkey || rec.author != me {
            return Err(NodeError::Invalid("message non renvoyable"));
        }
        if rec.deleted {
            return Err(NodeError::Invalid("message supprimé"));
        }
        if rec.acked {
            return Err(NodeError::Invalid("message déjà livré"));
        }
        // Retire toute copie encore en file pour repartir d'un backoff neuf : la
        // réémission ci-dessous en recréera une si le pair est injoignable.
        self.outbox_ack(peer_pubkey, msg_id)?;
        let msg = CoreMsg::DirectMsg {
            msg_id: rec.msg_id,
            lamport: rec.lamport,
            sent_ms: rec.sent_ms,
            kind: rec.kind,
            body: rec.body,
        };
        self.outbound.send(Outbound::Core {
            to: *peer_pubkey,
            msg: Box::new(msg),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use accord_core::db::Db;
    use accord_crypto::Identity;
    use accord_proto::core_msg::MsgBody;
    use tokio::sync::mpsc;

    use super::*;
    use crate::outbound::OutboundSink;

    /// Node wired to an outbound channel, with one established friend that
    /// already sent us a text message (lamport of that message returned).
    fn node_with_incoming_dm() -> (Node, [u8; 32], u64, mpsc::Receiver<Outbound>) {
        let id = Identity::generate_with_pow_bits(1);
        let db = Db::open_in_memory(&[1u8; 32]).unwrap();
        let (sink, mut rx) = OutboundSink::channel(64);
        let node = Node::new(id, db, sink);
        let peer = Identity::generate_with_pow_bits(1);
        node.friend_request(&peer.public_key(), "Pair").unwrap();
        node.ingest_core(
            &peer.public_key(),
            CoreMsg::FriendResponse { accepted: true },
        )
        .unwrap();
        let body = MsgBody::Text {
            text: "coucou".into(),
            reply_to: None,
            attachments: vec![],
        };
        let lamport = 7;
        node.ingest_core(
            &peer.public_key(),
            CoreMsg::DirectMsg {
                msg_id: [9; 16],
                lamport,
                sent_ms: 1_000,
                kind: body.kind(),
                body: body.encode_body(),
            },
        )
        .unwrap();
        while rx.try_recv().is_ok() {}
        (node, peer.public_key(), lamport, rx)
    }

    /// Next outgoing `DirectMsg` of the given body kind, if any.
    fn next_dm_of_kind(rx: &mut mpsc::Receiver<Outbound>, wanted: u8) -> Option<CoreMsg> {
        while let Ok(action) = rx.try_recv() {
            if let Outbound::Core { msg, .. } = action {
                if matches!(*msg, CoreMsg::DirectMsg { kind, .. } if kind == wanted) {
                    return Some(*msg);
                }
            }
        }
        None
    }

    /// Vide le canal sortant en une passe. Une photo unique, parce qu'une
    /// assertion qui cherche un message consommerait sinon celui que la
    /// suivante attend — et le test passerait pour de mauvaises raisons.
    fn sortants(rx: &mut mpsc::Receiver<Outbound>) -> Vec<([u8; 32], CoreMsg)> {
        let mut out = Vec::new();
        while let Ok(action) = rx.try_recv() {
            if let Outbound::Core { to, msg } = action {
                out.push((to, *msg));
            }
        }
        out
    }

    /// Première marque de lecture de compte sortante, avec son destinataire.
    fn marque_sortante(rx: &mut mpsc::Receiver<Outbound>) -> Option<([u8; 32], CoreMsg)> {
        sortants(rx)
            .into_iter()
            .find(|(_, m)| matches!(m, CoreMsg::SelfReadMark { .. }))
    }

    /// Inscrit `appareil` dans la liste signée du compte de `node`, comme le
    /// ferait un appairage abouti.
    fn autorise_appareil(node: &Node, appareil: &accord_crypto::DeviceIdentity) {
        let liste = crate::device::build_device_list_with_root(
            &node.identity,
            appareil,
            "Portable",
            now_ms(),
            accord_proto::device::DEVICE_FLAG_TRANSPORT_KEY,
        );
        node.store_device_list(&liste).unwrap();
    }

    /// Fait ingérer à `node` un message texte du pair.
    fn texte_du_pair(node: &Node, peer: &[u8; 32], msg_id: [u8; 16], lamport: u64) {
        let body = MsgBody::Text {
            text: "encore".into(),
            reply_to: None,
            attachments: vec![],
        };
        node.ingest_core(
            peer,
            CoreMsg::DirectMsg {
                msg_id,
                lamport,
                sent_ms: 2_000,
                kind: body.kind(),
                body: body.encode_body(),
            },
        )
        .unwrap();
    }

    /// Marque de lecture de compte visant `peer` jusqu'au message `up_to`.
    fn marque(peer: [u8; 32], up_to: [u8; 16]) -> CoreMsg {
        CoreMsg::SelfReadMark {
            scope: SELF_READ_SCOPE_DM,
            conv: peer,
            up_to,
        }
    }

    #[test]
    fn mark_read_sends_receipt_to_online_peer_once() {
        let (node, peer, lamport, mut rx) = node_with_incoming_dm();
        // Peer is presumed online (their message was ingested).
        node.dm_mark_read(&peer, lamport).unwrap();
        let msg = next_dm_of_kind(&mut rx, 6).expect("accusé de lecture attendu");
        match msg {
            CoreMsg::DirectMsg { kind, body, .. } => {
                assert_eq!(kind, 6);
                assert_eq!(
                    MsgBody::decode_body(kind, &body).unwrap(),
                    MsgBody::ReadReceipt { up_to: [9; 16] }
                );
            }
            other => panic!("message inattendu : {other:?}"),
        }
        // Throttle: re-marking the same position emits nothing.
        node.dm_mark_read(&peer, lamport).unwrap();
        assert!(next_dm_of_kind(&mut rx, 6).is_none());
    }

    #[test]
    fn mark_read_stays_silent_for_offline_peer() {
        let (node, peer, lamport, mut rx) = node_with_incoming_dm();
        node.ingest_core(
            &peer,
            CoreMsg::Presence {
                status: 3,
                custom: None,
            },
        )
        .unwrap();
        node.dm_mark_read(&peer, lamport).unwrap();
        assert!(next_dm_of_kind(&mut rx, 6).is_none());
        // The local read mark is persisted anyway (unread counter drops).
        assert_eq!(node.dm_unread(&peer).unwrap(), 0);
    }

    #[test]
    fn privacy_toggle_disables_outgoing_receipts_only() {
        let (node, peer, lamport, mut rx) = node_with_incoming_dm();
        assert!(node.read_receipts_enabled().unwrap());
        node.set_read_receipts(false).unwrap();
        assert!(!node.read_receipts_enabled().unwrap());

        node.dm_mark_read(&peer, lamport).unwrap();
        assert!(next_dm_of_kind(&mut rx, 6).is_none());

        // Incoming receipts are still recorded (peer read our message).
        let msg_id = {
            let hex_id = node.dm_send(&peer, "lu ?", None).unwrap();
            crate::hex::decode::<16>(&hex_id).unwrap()
        };
        let rr = MsgBody::ReadReceipt { up_to: msg_id };
        node.ingest_core(
            &peer,
            CoreMsg::DirectMsg {
                msg_id: [8; 16],
                lamport: 50,
                sent_ms: 2_000,
                kind: rr.kind(),
                body: rr.encode_body(),
            },
        )
        .unwrap();
        assert!(node.dm_peer_read_lamport(&peer).unwrap().is_some());

        // Re-enabling restores emission on the next advance.
        node.set_read_receipts(true).unwrap();
        node.dm_mark_read(&peer, lamport + 100).unwrap();
        assert!(next_dm_of_kind(&mut rx, 6).is_some());
    }

    #[test]
    fn pin_unpin_and_history_around_window() {
        let (node, peer, _lamport, _rx) = node_with_incoming_dm();
        let hex_id = node.dm_send(&peer, "à épingler", None).unwrap();
        let mid = crate::hex::decode::<16>(&hex_id).unwrap();
        // Pinning an unknown message fails; a known one succeeds (idempotent).
        assert!(node.dm_pin(&peer, &[0xEE; 16]).is_err());
        node.dm_pin(&peer, &mid).unwrap();
        assert_eq!(node.dm_pins(&peer).unwrap(), vec![hex_id]);
        assert!(node.dm_pinned_set(&peer).unwrap().contains(&mid));
        node.dm_unpin(&peer, &mid).unwrap();
        assert!(node.dm_pins(&peer).unwrap().is_empty());

        // history_around centers on the target; unknown id ⇒ found = false.
        let (window, found) = node.dm_history_around(&peer, &mid, 10).unwrap();
        assert!(found && window.iter().any(|m| m.msg_id == mid));
        let (empty, found) = node.dm_history_around(&peer, &[0xEE; 16], 10).unwrap();
        assert!(!found && empty.is_empty());
    }

    #[test]
    fn delivery_states_and_retry_reemits() {
        let (node, peer, _lamport, mut rx) = node_with_incoming_dm();
        let hex_id = node.dm_send(&peer, "coucou", None).unwrap();
        let mid = crate::hex::decode::<16>(&hex_id).unwrap();
        while rx.try_recv().is_ok() {}
        let rec = || {
            node.dm_history(&peer, u64::MAX, 10)
                .unwrap()
                .into_iter()
                .find(|m| m.msg_id == mid)
                .unwrap()
        };
        // Not queued, fresh ⇒ pending.
        let empty: HashMap<[u8; 16], (u32, bool)> = HashMap::new();
        let jamais_rattrape: HashSet<[u8; 16]> = HashSet::new();
        assert_eq!(
            node.dm_delivery(&rec(), &empty, &jamais_rattrape),
            "pending"
        );
        // Exhausted direct retries ⇒ failed.
        let mut map = HashMap::new();
        map.insert(mid, (DM_FAILED_ATTEMPTS, false));
        assert_eq!(node.dm_delivery(&rec(), &map, &jamais_rattrape), "failed");

        // Retry re-emits the same DirectMsg (kind 0 = Text).
        node.dm_retry(&peer, &mid).unwrap();
        match next_dm_of_kind(&mut rx, 0).expect("réémission attendue") {
            CoreMsg::DirectMsg { msg_id, .. } => assert_eq!(msg_id, mid),
            other => panic!("message inattendu : {other:?}"),
        }

        // Acked ⇒ sent; retrying a delivered message is refused.
        node.ingest_core(&peer, CoreMsg::MsgAck { msg_id: mid })
            .unwrap();
        assert_eq!(node.dm_delivery(&rec(), &map, &jamais_rattrape), "sent");
        assert!(node.dm_retry(&peer, &mid).is_err());
    }

    #[test]
    fn la_marque_de_lecture_avance_mais_ne_recule_jamais() {
        let (node, peer, lamport, _rx) = node_with_incoming_dm();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), 0);

        // Une marque plus récente avance.
        node.dm_mark_read(&peer, lamport).unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), lamport);
        node.dm_mark_read(&peer, lamport + 10).unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), lamport + 10);

        // Une marque plus ancienne est ignorée en silence : pas d'erreur, c'est
        // simplement un appareil du compte resté en arrière.
        node.dm_mark_read(&peer, lamport).unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), lamport + 10);

        // Une marque identique ne change rien (re-marquage au refocus).
        node.dm_mark_read(&peer, lamport + 10).unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), lamport + 10);
    }

    #[test]
    fn deux_appareils_dont_un_en_retard_retiennent_la_marque_la_plus_avancee() {
        // « Lu = lu sur au moins un appareil » (MULTI_DEVICE.md §5) : quel que
        // soit l'ordre d'arrivée, la marque retenue est la plus avancée des
        // deux. L'appareil en retard se place avant le message entrant, donc
        // une régression rendrait la conversation non lue — le symptôme exact
        // que la monotonie protège.
        let a_jour = |base: u64| base + 10;
        let en_retard = |base: u64| base - 4;

        // L'appareil en retard parle en dernier : sa position est ignorée.
        let (node, peer, lamport, _rx) = node_with_incoming_dm();
        node.dm_mark_read(&peer, a_jour(lamport)).unwrap();
        node.dm_mark_read(&peer, en_retard(lamport)).unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), a_jour(lamport));
        assert_eq!(node.dm_unread(&peer).unwrap(), 0);

        // Ordre inverse : l'appareil à jour rattrape et l'emporte.
        let (node, peer, lamport, _rx) = node_with_incoming_dm();
        node.dm_mark_read(&peer, en_retard(lamport)).unwrap();
        node.dm_mark_read(&peer, a_jour(lamport)).unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), a_jour(lamport));
        assert_eq!(node.dm_unread(&peer).unwrap(), 0);
    }

    #[test]
    fn un_accuse_de_lecture_perime_ne_recule_pas_la_position_du_pair() {
        let (node, peer, _lamport, _rx) = node_with_incoming_dm();
        // Deux messages sortants : le pair pourra accuser l'un puis l'autre.
        let premier = crate::hex::decode::<16>(&node.dm_send(&peer, "un", None).unwrap()).unwrap();
        let second = crate::hex::decode::<16>(&node.dm_send(&peer, "deux", None).unwrap()).unwrap();
        let position = |id: &[u8; 16]| {
            node.dm_history(&peer, u64::MAX, 10)
                .unwrap()
                .into_iter()
                .find(|m| m.msg_id == *id)
                .unwrap()
                .lamport
        };
        let ingest_receipt = |up_to: [u8; 16], msg_id: [u8; 16], lamport: u64| {
            let rr = MsgBody::ReadReceipt { up_to };
            node.ingest_core(
                &peer,
                CoreMsg::DirectMsg {
                    msg_id,
                    lamport,
                    sent_ms: 4_000,
                    kind: rr.kind(),
                    body: rr.encode_body(),
                },
            )
            .unwrap();
        };

        ingest_receipt(second, [21; 16], position(&second) + 1);
        assert_eq!(
            node.dm_peer_read_lamport(&peer).unwrap(),
            Some(position(&second))
        );

        // Le second appareil du pair, en retard, accuse un message plus ancien :
        // sans monotonie, l'expéditeur verrait « deux » redevenir non lu.
        ingest_receipt(premier, [22; 16], position(&second) + 2);
        assert_eq!(
            node.dm_peer_read_lamport(&peer).unwrap(),
            Some(position(&second))
        );
    }

    #[test]
    fn peer_read_lamport_maps_receipt_to_conversation_position() {
        let (node, peer, _lamport, _rx) = node_with_incoming_dm();
        assert_eq!(node.dm_peer_read_lamport(&peer).unwrap(), None);
        let hex_id = node.dm_send(&peer, "à lire", None).unwrap();
        let msg_id = crate::hex::decode::<16>(&hex_id).unwrap();
        let sent_lamport = node.dm_history(&peer, u64::MAX, 1).unwrap()[0].lamport;
        let rr = MsgBody::ReadReceipt { up_to: msg_id };
        node.ingest_core(
            &peer,
            CoreMsg::DirectMsg {
                msg_id: [7; 16],
                lamport: sent_lamport + 1,
                sent_ms: 3_000,
                kind: rr.kind(),
                body: rr.encode_body(),
            },
        )
        .unwrap();
        assert_eq!(
            node.dm_peer_read_lamport(&peer).unwrap(),
            Some(sent_lamport)
        );
    }

    #[test]
    fn marquer_lu_annonce_la_position_a_notre_propre_compte() {
        let (node, peer, lamport, mut rx) = node_with_incoming_dm();
        // Un message à nous, POSTÉRIEUR à celui du pair : c'est le lamport que
        // l'interface passera, et il vient de NOTRE horloge.
        node.dm_send(&peer, "réponse", None).unwrap();
        while rx.try_recv().is_ok() {}

        node.dm_mark_read(&peer, u64::MAX).unwrap();
        let (to, msg) = marque_sortante(&mut rx).expect("marque de compte attendue");
        assert_eq!(
            to,
            node.public_key(),
            "adressée au COMPTE : c'est la couche réseau qui la ventile par appareil"
        );
        assert_eq!(
            msg,
            marque(peer, [9; 16]),
            "ramenée au dernier message DU PAIR, jamais au nôtre ni à un lamport"
        );

        // Re-marquer la même position n'annonce rien (retour de focus).
        node.dm_mark_read(&peer, u64::MAX).unwrap();
        assert!(marque_sortante(&mut rx).is_none());
        assert_eq!(lamport, 7);
    }

    #[test]
    fn couper_les_accuses_de_lecture_ne_coupe_pas_la_synchro_de_nos_appareils() {
        // 🔒 Le réglage promet quelque chose au sujet du PAIR. S'il coupait au
        // passage la synchronisation entre les machines de l'utilisateur, une
        // option de confidentialité casserait en douce une fonction sans
        // rapport — et rien à l'écran ne le dirait.
        let (node, peer, lamport, mut rx) = node_with_incoming_dm();
        node.set_read_receipts(false).unwrap();
        node.dm_mark_read(&peer, lamport).unwrap();

        let envois = sortants(&mut rx);
        assert!(
            !envois
                .iter()
                .any(|(_, m)| matches!(m, CoreMsg::DirectMsg { kind: 6, .. })),
            "le pair ne doit rien apprendre : c'est ce que le réglage promet"
        );
        assert!(
            envois.iter().any(
                |(to, m)| *to == node.public_key() && matches!(m, CoreMsg::SelfReadMark { .. })
            ),
            "nos propres appareils, eux, restent synchronisés"
        );
    }

    #[test]
    fn une_marque_dun_appareil_du_compte_avance_la_position() {
        let (node, peer, lamport, _rx) = node_with_incoming_dm();
        let appareil = accord_crypto::DeviceIdentity::generate_with_pow_bits(1);
        autorise_appareil(&node, &appareil);
        assert_eq!(node.dm_unread(&peer).unwrap(), 1);

        // Phase 2 : l'appareil présente SA clé, inscrite dans notre liste.
        node.ingest_core(&appareil.public_key(), marque(peer, [9; 16]))
            .unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), lamport);
        assert_eq!(node.dm_unread(&peer).unwrap(), 0);

        // Phase 1 et parc mixte : l'appareil non basculé présente la clé de
        // COMPTE. Nul ne peut la présenter sans détenir la graine du compte,
        // donc sans être une de nos machines — elle doit être acceptée, sans
        // quoi la moitié du parc mixte ne synchroniserait rien.
        let (node, peer, lamport, _rx) = node_with_incoming_dm();
        autorise_appareil(&node, &appareil);
        node.ingest_core(&node.public_key(), marque(peer, [9; 16]))
            .unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), lamport);
        assert_eq!(node.dm_unread(&peer).unwrap(), 0);
    }

    #[test]
    fn une_marque_dune_cle_non_autorisee_ne_change_rien() {
        // 🔒 Le seul contrôle qui protège le badge : sans lui, n'importe quel
        // ami éteindrait n'importe quelle conversation à distance.
        let (node, peer, _lamport, _rx) = node_with_incoming_dm();
        let appareil = accord_crypto::DeviceIdentity::generate_with_pow_bits(1);
        autorise_appareil(&node, &appareil);

        let etranger = accord_crypto::DeviceIdentity::generate_with_pow_bits(1);
        // L'ami lui-même est le cas qui compte : c'est celui dont on reçoit
        // vraiment des messages, donc celui qui peut vraiment essayer.
        for intrus in [peer, etranger.public_key(), [0x7E; 32]] {
            node.ingest_core(&intrus, marque(peer, [9; 16])).unwrap();
            assert_eq!(
                node.dm_read_lamport(&peer).unwrap(),
                0,
                "clé non autorisée : la position ne doit pas bouger"
            );
            assert_eq!(node.dm_unread(&peer).unwrap(), 1);
        }

        // Un appareil RÉVOQUÉ redevient un étranger : c'est la liste COURANTE
        // qui fait foi, pas celle du jour de l'appairage. Une machine volée
        // doit cesser de piloter les badges du compte.
        let mut liste = crate::device::build_device_list_with_root(
            &node.identity,
            &appareil,
            "Portable",
            now_ms(),
            accord_proto::device::DEVICE_FLAG_TRANSPORT_KEY,
        );
        liste.revoked.push(accord_proto::device::RevokedEntry {
            pubkey: appareil.public_key(),
            revoked_ms: now_ms(),
        });
        accord_crypto::sign_device_list_with_root(&node.identity, &mut liste);
        node.store_device_list(&liste).unwrap();
        node.ingest_core(&appareil.public_key(), marque(peer, [9; 16]))
            .unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), 0);
        assert_eq!(node.dm_unread(&peer).unwrap(), 1);
    }

    #[test]
    fn une_marque_plus_ancienne_ne_fait_pas_reculer_la_position() {
        // Un appareil resté trois jours éteint rejoue sa vieille position en
        // se rallumant. Sans monotonie, la conversation redeviendrait non lue
        // toute seule sur la machine qui, elle, était à jour.
        let (node, peer, lamport, _rx) = node_with_incoming_dm();
        let appareil = accord_crypto::DeviceIdentity::generate_with_pow_bits(1);
        autorise_appareil(&node, &appareil);
        texte_du_pair(&node, &peer, [10; 16], lamport + 5);

        node.ingest_core(&appareil.public_key(), marque(peer, [10; 16]))
            .unwrap();
        assert_eq!(node.dm_read_lamport(&peer).unwrap(), lamport + 5);
        assert_eq!(node.dm_unread(&peer).unwrap(), 0);

        node.ingest_core(&appareil.public_key(), marque(peer, [9; 16]))
            .unwrap();
        assert_eq!(
            node.dm_read_lamport(&peer).unwrap(),
            lamport + 5,
            "une position en retard est écartée, pas appliquée"
        );
        assert_eq!(node.dm_unread(&peer).unwrap(), 0);
    }

    #[test]
    fn une_marque_ne_couvre_pas_un_message_du_pair_jamais_recu_ici() {
        // Deux machines d'un même compte, amies du même pair. Le bureau a reçu
        // deux messages, le portable un seul — un portable resté éteint une
        // journée. La marque du bureau ne doit pas faire passer pour lu, sur
        // le portable, un message qu'il n'a jamais vu : il l'afficherait alors
        // en gris à sa prochaine synchronisation, et l'utilisateur ne saurait
        // jamais qu'il existe.
        let (bureau, peer, lamport, mut rx) = node_with_incoming_dm();
        let graine = *bureau.identity.seed();
        texte_du_pair(&bureau, &peer, [10; 16], lamport + 5);

        let (sink, _rx2) = OutboundSink::channel(64);
        let portable = Node::new(
            Identity::from_seed_with_pow_bits(graine, 1),
            Db::open_in_memory(&[2u8; 32]).unwrap(),
            sink,
        );
        assert_eq!(portable.public_key(), bureau.public_key());
        portable.friend_request(&peer, "Pair").unwrap();
        portable
            .ingest_core(&peer, CoreMsg::FriendResponse { accepted: true })
            .unwrap();
        texte_du_pair(&portable, &peer, [9; 16], lamport);
        assert_eq!(portable.dm_unread(&peer).unwrap(), 1);

        // Le bureau marque tout lu et diffuse.
        while rx.try_recv().is_ok() {}
        bureau.dm_mark_read(&peer, u64::MAX).unwrap();
        let (_, diffusee) = marque_sortante(&mut rx).expect("marque de compte attendue");
        assert_eq!(diffusee, marque(peer, [10; 16]));

        portable
            .ingest_core(&bureau.public_key(), diffusee.clone())
            .unwrap();
        assert_eq!(
            portable.dm_read_lamport(&peer).unwrap(),
            0,
            "identifiant inconnu ici : rien n'avance"
        );
        assert_eq!(portable.dm_unread(&peer).unwrap(), 1);

        // Le message rattrapé, la même marque rejouée l'éteint enfin : le
        // refus ci-dessus était bien un report, pas une perte.
        texte_du_pair(&portable, &peer, [10; 16], lamport + 5);
        portable
            .ingest_core(&bureau.public_key(), diffusee)
            .unwrap();
        assert_eq!(portable.dm_read_lamport(&peer).unwrap(), lamport + 5);
        assert_eq!(portable.dm_unread(&peer).unwrap(), 0);
    }
}
