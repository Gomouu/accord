//! Rattrapage des conversations directes entre les appareils d'un même compte
//! (jalon 1, lot 1.E, tâche 4 ; `docs/MULTI_DEVICE.md` §7 étape 2).
//!
//! Un appareil qui se rallume offre, conversation par conversation, l'empreinte
//! de ce qu'il détient ; ses autres machines comparent, demandent ce qui leur
//! manque, et le reçoivent message par message. Le mécanisme filaire est décrit
//! sur [`CoreMsg::SelfSyncOffer`], la comparaison dans
//! [`accord_core::dm_sync`] ; ce module porte ce qui relève du nœud :
//! l'autorisation, le curseur persisté et l'émission.

use accord_core::db::DmRecord;
use accord_core::dm_sync::{self, SyncOffer};
use accord_proto::core_msg::{CoreMsg, MAX_SELF_SYNC_ITEMS};
use serde_json::json;

use crate::error::NodeError;
use crate::hex;

use super::{read_u64, Node};

/// Conversations couvertes par une passe d'offres, des plus récemment actives
/// aux plus anciennes.
///
/// ⚠️ Un compte à trois cents contacts émettrait autrement trois cents
/// datagrammes par appareil et par reconnexion, pour des conversations dont
/// l'immense majorité n'a rien bougé. Ce qui tombe hors de la fenêtre n'est pas
/// perdu : il remonte dès qu'un message y arrive et la fait revenir en tête.
const SELF_SYNC_MAX_CONVERSATIONS: usize = 64;

/// Clé de métadonnée du curseur de rattrapage : **par appareil frère ET par
/// conversation**.
///
/// 🔒 Par appareil, parce qu'un troisième appareil qui se réveille très en
/// retard ne doit pas faire reculer le point déjà réglé avec les deux autres —
/// un curseur unique le ferait, et l'on rejouerait alors le même paquet de
/// messages à chaque passe, indéfiniment.
///
/// ⚠️ Par conversation, en plus : les positions de Lamport ne sont **pas**
/// comparables d'une conversation à l'autre. Un message reçu porte l'horloge du
/// pair qui l'a écrit ; une position de 900 dans une conversation ne dit
/// strictement rien d'une conversation dont les positions tournent autour de 5.
/// Un curseur unique par appareil, appliqué à toutes les conversations, en
/// affamerait donc silencieusement une partie.
fn cursor_key(device: &[u8; 32], conv: &[u8; 32]) -> String {
    format!("dmsync:{}:{}", hex::encode(device), hex::encode(conv))
}

impl Node {
    /// Offres de rattrapage à émettre vers nos autres appareils : une par
    /// conversation active, bornée à [`SELF_SYNC_MAX_CONVERSATIONS`].
    pub fn self_sync_offers(&self) -> Result<Vec<CoreMsg>, NodeError> {
        self.with_db(|db| {
            let mut offres = Vec::new();
            for conv in db.dm_conversations(SELF_SYNC_MAX_CONVERSATIONS)? {
                let offer = dm_sync::sync_offer(db, &conv)?;
                offres.push(offer_msg(&offer));
            }
            Ok(offres)
        })
    }

    /// Ingère l'offre d'un de nos appareils et demande, le cas échéant, ce qui
    /// manque ici.
    ///
    /// Silencieuse dans tous les cas de refus, comme
    /// [`Node::ingest_self_read_mark`] : une offre d'une clé qui n'est pas des
    /// nôtres n'est pas une erreur à remonter, seulement une offre sans effet.
    pub(super) fn ingest_self_sync_offer(
        &self,
        device: &[u8; 32],
        remote: SyncOffer,
    ) -> Result<Vec<CoreMsg>, NodeError> {
        if !self.is_own_listed_device(device) {
            return Ok(vec![]);
        }
        let local = self.with_db(|db| Ok(dm_sync::sync_offer(db, &remote.conv)?))?;
        if !dm_sync::diverges(&local, &remote) {
            // Rien à tirer : on note que tout est réglé avec CET appareil
            // jusqu'en haut de la fenêtre. Sans cette avance, la passe suivante
            // repartirait du vieux curseur et redemanderait une fenêtre entière
            // de messages déjà présents.
            self.advance_self_sync_cursor(device, &remote.conv, local.max_lamport)?;
            return Ok(vec![]);
        }
        Ok(vec![CoreMsg::SelfSyncPull {
            conv: remote.conv,
            since_lamport: self.self_sync_cursor(device, &remote.conv)?,
            max_items: MAX_SELF_SYNC_ITEMS,
        }])
    }

    /// Sert une demande de rattrapage d'un de nos appareils.
    ///
    /// 🔒 C'est **le sens dangereux** : il livre de l'historique. Il exige donc
    /// plus que [`Node::ingest_self_read_mark`] — la clé demandeuse doit figurer
    /// dans notre liste d'appareils COURANTE, et la clé de compte n'y suffit
    /// pas. Voir [`Node::is_own_listed_device`] pour ce que cette différence
    /// achète, et ce qu'elle coûte.
    pub(super) fn ingest_self_sync_pull(
        &self,
        device: &[u8; 32],
        conv: &[u8; 32],
        since_lamport: u64,
        max_items: u16,
    ) -> Result<Vec<CoreMsg>, NodeError> {
        if !self.is_own_listed_device(device) {
            return Ok(vec![]);
        }
        // Le décodage a déjà borné `max_items` à [`MAX_SELF_SYNC_ITEMS`] ; le
        // `min` ne fait que rendre la borne visible ici, là où l'on sert.
        let combien = usize::from(max_items.min(MAX_SELF_SYNC_ITEMS));
        let items =
            self.with_db(|db| Ok(dm_sync::items_for_pull(db, conv, since_lamport, combien)?))?;
        Ok(items.iter().map(|m| item_msg(conv, m)).collect())
    }

    /// Ingère un message d'historique servi par un de nos appareils.
    pub(super) fn ingest_self_sync_item(
        &self,
        device: &[u8; 32],
        record: &DmRecord,
    ) -> Result<(), NodeError> {
        if !self.is_own_listed_device(device) {
            return Ok(());
        }
        // 🔒 Un message d'une conversation n'a que deux auteurs possibles : le
        // pair, ou nous. Sans ce contrôle, un appareil du compte pourrait
        // classer dans la conversation de P un message signé du nom de Q — et
        // l'interface l'afficherait comme tel, sans que rien ne le démente.
        if record.author != record.peer && record.author != self.public_key() {
            return Ok(());
        }
        // 🔒 Et seulement dans une conversation qui existe déjà pour nous : le
        // rattrapage remplit un historique, il ne crée pas de relation.
        if !self.is_relation(&record.peer) {
            return Ok(());
        }
        let insere = self.with_db(|db| Ok(dm_sync::ingest_item(db, &self.search_key, record)?))?;
        // Le curseur avance même sur un doublon : ce qui compte est ce que cet
        // appareil nous a DONNÉ, pas ce que nous en avons fait.
        self.advance_self_sync_cursor(device, &record.peer, record.lamport)?;
        if insere {
            let attachments = self.with_db(|db| Ok(db.msg_attachments(&record.msg_id)?))?;
            self.emit(
                "event.dm",
                json!({
                    "peer": hex::encode(&record.peer),
                    "msg_id": hex::encode(&record.msg_id),
                    "attachments": super::dm::attachments_json(&attachments),
                }),
            );
        }
        Ok(())
    }

    /// Position déjà réglée avec `device` pour `conv` (0 si jamais rattrapé).
    fn self_sync_cursor(&self, device: &[u8; 32], conv: &[u8; 32]) -> Result<u64, NodeError> {
        let key = cursor_key(device, conv);
        self.with_db(|db| Ok(read_u64(db.meta(&key)?)))
    }

    /// Avance le curseur de rattrapage, jamais ne le fait reculer.
    ///
    /// La monotonie est ce qui empêche une passe tronquée de repartir de zéro à
    /// la suivante : on reprend là où l'on s'est arrêté, sans trou puisque les
    /// éléments sont servis dans l'ordre croissant.
    fn advance_self_sync_cursor(
        &self,
        device: &[u8; 32],
        conv: &[u8; 32],
        position: u64,
    ) -> Result<(), NodeError> {
        let key = cursor_key(device, conv);
        self.with_db(|db| {
            if position > read_u64(db.meta(&key)?) {
                db.set_meta(&key, &position.to_be_bytes())?;
            }
            Ok(())
        })
    }

    /// Vrai si `key` figure dans la liste d'appareils COURANTE du compte.
    ///
    /// 🔒 Plus strict que [`Node::is_own_device`], et la différence est le seul
    /// verrou qui protège l'historique : la clé de **compte** n'est pas acceptée
    /// ici. Elle l'est pour une marque de lecture parce que nul ne peut la
    /// présenter au transport sans détenir la graine du compte — mais un
    /// appareil **révoqué** détient encore cette graine (tant qu'elle est
    /// partagée entre les machines d'un compte), et l'accepter reviendrait à
    /// laisser une machine volée retirer, conversation par conversation, tout
    /// l'historique récent du compte. Ce qu'une liste signée dit, elle, c'est
    /// qui est autorisé **maintenant** : c'est précisément ce que la révocation
    /// modifie.
    ///
    /// Ce que cette rigueur coûte : un appareil qui n'a pas encore basculé son
    /// transport se présente sous la clé de compte et ne peut donc pas
    /// rattraper. Coût nul en pratique — deux appareils non basculés présentent
    /// la même clé et s'évincent l'un l'autre au transport (§1), il n'existe
    /// donc pas de scénario à deux appareils avant le basculement.
    ///
    /// ⚠️ Pas de rafraîchissement réseau avant de servir, et ce n'est pas un
    /// oubli : il n'existe aujourd'hui **aucun chemin** par lequel la liste de
    /// notre propre compte se rafraîchit depuis le réseau
    /// (`Node::ingest_device_list` exige un ami, `ingest_device_list_record` une
    /// relation — on n'est ni l'un ni l'autre de soi-même), et l'ingestion est
    /// synchrone, donc incapable d'attendre une réponse DHT. Ce qui est fait ici
    /// est ce qui est faisable : relire la liste PERSISTÉE à chaque demande,
    /// jamais un cache mémoire, et refuser dès qu'elle est illisible.
    fn is_own_listed_device(&self, key: &[u8; 32]) -> bool {
        // Une liste illisible ne prouve rien : on refuse, comme
        // `DeviceList::authorises` refuse une liste incohérente.
        self.current_device_list()
            .map(|list| list.authorises(key))
            .unwrap_or(false)
    }
}

/// Enveloppe filaire d'une offre.
fn offer_msg(offer: &SyncOffer) -> CoreMsg {
    CoreMsg::SelfSyncOffer {
        conv: offer.conv,
        count: offer.count,
        max_lamport: offer.max_lamport,
        digest: offer.digest,
    }
}

/// Enveloppe filaire d'un message d'historique.
///
/// 🔴 Porte le `msg_id` d'origine et l'auteur réel — voir
/// [`CoreMsg::SelfSyncItem`] pour les deux pannes silencieuses qu'une
/// réémission de `DirectMsg` provoquerait à la place.
fn item_msg(conv: &[u8; 32], m: &DmRecord) -> CoreMsg {
    CoreMsg::SelfSyncItem {
        conv: *conv,
        msg_id: m.msg_id,
        author: m.author,
        lamport: m.lamport,
        sent_ms: m.sent_ms,
        kind: m.kind,
        body: m.body.clone(),
        acked: m.acked,
        deleted: m.deleted,
        edited: m.edited.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use accord_core::db::Db;
    use accord_crypto::{DeviceIdentity, Identity};
    use accord_proto::core_msg::MsgBody;
    use accord_proto::device::{DeviceEntry, DeviceList, RevokedEntry, DEVICE_FLAG_TRANSPORT_KEY};

    use super::*;
    use crate::node::now_ms;
    use crate::outbound::OutboundSink;

    /// Une machine du compte : le nœud et la clé de transport qu'il présente.
    struct Appareil {
        node: Node,
        device: DeviceIdentity,
    }

    impl Appareil {
        fn key(&self) -> [u8; 32] {
            self.device.public_key()
        }
    }

    /// Liste signée du compte listant `appareils`, telle que la produirait une
    /// suite d'appairages aboutis.
    fn liste(root: &Identity, appareils: &[&DeviceIdentity]) -> DeviceList {
        let mut list = DeviceList {
            account: root.public_key(),
            version: accord_crypto::version_for(now_ms()),
            issued_ms: now_ms(),
            valid_for_s: crate::device::DEVICE_LIST_VALID_S,
            devices: appareils
                .iter()
                .map(|d| DeviceEntry {
                    pubkey: d.public_key(),
                    pow_nonce: d.pow_nonce(),
                    name: "Machine".into(),
                    added_ms: now_ms(),
                    flags: DEVICE_FLAG_TRANSPORT_KEY,
                })
                .collect(),
            revoked: Vec::new(),
            sig: [0u8; 64],
        };
        accord_crypto::sign_device_list_with_root(root, &mut list);
        list
    }

    /// Deux machines d'UN MÊME compte (même graine racine, clés d'appareil
    /// distinctes), amies du même pair, chacune reconnaissant l'autre dans sa
    /// liste signée. Rend aussi la clé de compte du pair.
    fn deux_appareils() -> (Appareil, Appareil, [u8; 32]) {
        let racine = Identity::generate_with_pow_bits(1);
        let graine = *racine.seed();
        let pair = Identity::generate_with_pow_bits(1).public_key();
        let bureau_dev = DeviceIdentity::generate_with_pow_bits(1);
        let portable_dev = DeviceIdentity::generate_with_pow_bits(1);

        let monte = |sel: u8, device: DeviceIdentity| {
            let (sink, _rx) = OutboundSink::channel(64);
            let node = Node::new(
                Identity::from_seed_with_pow_bits(graine, 1),
                Db::open_in_memory(&[sel; 32]).unwrap(),
                sink,
            );
            node.friend_request(&pair, "Pair").unwrap();
            node.ingest_core(&pair, CoreMsg::FriendResponse { accepted: true })
                .unwrap();
            Appareil { node, device }
        };
        let bureau = monte(1, bureau_dev);
        let portable = monte(2, portable_dev);
        for a in [&bureau, &portable] {
            a.node
                .store_device_list(&liste(
                    &a.node.identity,
                    &[&bureau.device, &portable.device],
                ))
                .unwrap();
        }
        assert_eq!(bureau.node.public_key(), portable.node.public_key());
        (bureau, portable, pair)
    }

    /// Écrit directement une ligne d'historique dans la base de `a`, sans
    /// passer par la composition : les tests ont besoin d'auteurs et
    /// d'horodatages choisis.
    fn ecrit(a: &Appareil, conv: [u8; 32], author: [u8; 32], id: u8, lamport: u64, sent_ms: u64) {
        let body = MsgBody::Text {
            text: format!("message {id}"),
            reply_to: None,
            attachments: vec![],
        };
        a.node
            .with_db(|db| {
                db.insert_dm(&accord_core::db::DmRecord {
                    msg_id: [id; 16],
                    peer: conv,
                    author,
                    lamport,
                    sent_ms,
                    kind: body.kind(),
                    body: body.encode_body(),
                    acked: false,
                    deleted: false,
                    edited: None,
                })?;
                Ok(())
            })
            .unwrap();
    }

    /// Une passe complète de rattrapage de `source` vers `cible` : la source
    /// offre, la cible compare et tire, la source sert, la cible ingère. Rend
    /// le nombre d'éléments réellement transportés.
    fn passe(source: &Appareil, cible: &Appareil) -> usize {
        let mut transportes = 0usize;
        for offre in source.node.self_sync_offers().unwrap() {
            for tirage in cible.node.ingest_core(&source.key(), offre).unwrap() {
                for item in source.node.ingest_core(&cible.key(), tirage).unwrap() {
                    transportes += 1;
                    cible.node.ingest_core(&source.key(), item).unwrap();
                }
            }
        }
        transportes
    }

    /// Identifiants de l'historique d'une conversation, triés.
    fn ids(a: &Appareil, conv: &[u8; 32]) -> Vec<[u8; 16]> {
        let mut v: Vec<[u8; 16]> = a
            .node
            .dm_history(conv, u64::MAX, 500)
            .unwrap()
            .into_iter()
            .map(|m| m.msg_id)
            .collect();
        v.sort_unstable();
        v
    }

    #[test]
    fn deux_appareils_convergent_sur_lunion_de_leurs_messages() {
        let (bureau, portable, pair) = deux_appareils();
        let moi = bureau.node.public_key();
        // Le bureau a une réponse à nous et un message du pair ; le portable
        // n'a qu'un autre message du pair, reçu pendant que l'autre dormait.
        ecrit(&bureau, pair, moi, 1, 10, 1_000);
        ecrit(&bureau, pair, pair, 2, 11, 1_100);
        ecrit(&portable, pair, pair, 3, 12, 1_200);

        // Les deux sens, comme sur le terrain : chacun offre, chacun tire.
        passe(&bureau, &portable);
        passe(&portable, &bureau);

        let union = vec![[1u8; 16], [2u8; 16], [3u8; 16]];
        assert_eq!(ids(&bureau, &pair), union);
        assert_eq!(ids(&portable, &pair), union);
    }

    #[test]
    fn un_rattrapage_repete_ne_duplique_aucun_message() {
        // 🔴 Le cœur de la tâche : l'`msg_id` transporté est celui d'ORIGINE,
        // donc l'insertion idempotente reconnaît le doublon. Composer un
        // message neuf à la place frapperait un identifiant neuf à chaque
        // passe, et la conversation doublerait de taille toutes les cinq
        // minutes sans que rien ne le signale.
        let (bureau, portable, pair) = deux_appareils();
        let moi = bureau.node.public_key();
        ecrit(&bureau, pair, moi, 1, 10, 1_000);
        ecrit(&bureau, pair, pair, 2, 11, 1_100);

        assert_eq!(
            passe(&bureau, &portable),
            2,
            "premier passage : tout arrive"
        );
        assert_eq!(
            portable
                .node
                .dm_history(&pair, u64::MAX, 500)
                .unwrap()
                .len(),
            2
        );

        for _ in 0..3 {
            passe(&bureau, &portable);
            assert_eq!(
                portable
                    .node
                    .dm_history(&pair, u64::MAX, 500)
                    .unwrap()
                    .len(),
                2,
                "un rattrapage rejoué ne doit RIEN ajouter"
            );
        }
        assert_eq!(ids(&portable, &pair), vec![[1u8; 16], [2u8; 16]]);
    }

    #[test]
    fn une_demande_dune_cle_non_autorisee_ne_rend_rien() {
        // 🔒 Servir un tirage livre de l'historique : c'est le sens dangereux.
        let (bureau, _portable, pair) = deux_appareils();
        ecrit(&bureau, pair, bureau.node.public_key(), 1, 10, 1_000);
        let tirage = CoreMsg::SelfSyncPull {
            conv: pair,
            since_lamport: 0,
            max_items: MAX_SELF_SYNC_ITEMS,
        };
        let etranger = DeviceIdentity::generate_with_pow_bits(1).public_key();

        for intrus in [
            pair,                     // l'ami : celui qui peut vraiment essayer
            etranger,                 // une machine inconnue
            bureau.node.public_key(), // 🔒 la clé de COMPTE ne suffit pas ici
        ] {
            assert!(
                bureau
                    .node
                    .ingest_core(&intrus, tirage.clone())
                    .unwrap()
                    .is_empty(),
                "aucun historique ne doit sortir vers une clé non listée"
            );
        }

        // Et un appareil RÉVOQUÉ redevient un étranger : c'est la liste
        // COURANTE qui décide, pas celle du jour de l'appairage.
        let revoque = DeviceIdentity::generate_with_pow_bits(1);
        let mut l = liste(&bureau.node.identity, &[&bureau.device, &revoque]);
        l.revoked.push(RevokedEntry {
            pubkey: revoque.public_key(),
            revoked_ms: now_ms(),
        });
        accord_crypto::sign_device_list_with_root(&bureau.node.identity, &mut l);
        bureau.node.store_device_list(&l).unwrap();
        assert!(bureau
            .node
            .ingest_core(&revoque.public_key(), tirage.clone())
            .unwrap()
            .is_empty());

        // Contrôle : la même demande d'un appareil listé, elle, sert.
        let autorise = DeviceIdentity::generate_with_pow_bits(1);
        bureau
            .node
            .store_device_list(&liste(&bureau.node.identity, &[&bureau.device, &autorise]))
            .unwrap();
        assert_eq!(
            bureau
                .node
                .ingest_core(&autorise.public_key(), tirage)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn un_message_de_nous_rattrape_ne_saffiche_pas_en_echec() {
        // ⚠️ Un message de NOUS arrive sur la machine qui rattrape sans la
        // moindre ligne d'outbox : elle ne l'a jamais envoyé et ne l'enverra
        // jamais. Sans marque d'origine, la seule ancienneté suffirait à
        // l'afficher en rouge.
        let (bureau, portable, pair) = deux_appareils();
        let moi = bureau.node.public_key();
        let vieux = now_ms() - 8 * 24 * 3600 * 1000;
        ecrit(&bureau, pair, moi, 1, 10, vieux);

        // Contrôle négatif : sur la machine qui l'a composé, ce même message,
        // non acquitté et sorti de la file, EST un échec.
        let vide: HashMap<[u8; 16], (u32, bool)> = HashMap::new();
        let rien: HashSet<[u8; 16]> = HashSet::new();
        let rec = |a: &Appareil| {
            a.node
                .dm_history(&pair, u64::MAX, 10)
                .unwrap()
                .into_iter()
                .find(|m| m.msg_id == [1u8; 16])
                .expect("message présent")
        };
        assert_eq!(
            bureau.node.dm_delivery(&rec(&bureau), &vide, &rien),
            "failed"
        );

        assert_eq!(passe(&bureau, &portable), 1);
        let rattrapes = portable.node.dm_synced_states(&pair).unwrap();
        assert!(rattrapes.contains(&[1u8; 16]));
        assert_eq!(
            portable
                .node
                .dm_delivery(&rec(&portable), &vide, &rattrapes),
            "pending",
            "la machine qui rattrape n'a rien à dire de la livraison"
        );
    }

    #[test]
    fn deux_appareils_daccord_ne_declenchent_aucun_tirage() {
        let (bureau, portable, pair) = deux_appareils();
        ecrit(&bureau, pair, pair, 1, 10, 1_000);
        ecrit(&portable, pair, pair, 1, 10, 1_000);

        let offres = bureau.node.self_sync_offers().unwrap();
        assert_eq!(offres.len(), 1, "une offre par conversation active");
        for offre in offres {
            assert!(
                portable
                    .node
                    .ingest_core(&bureau.key(), offre)
                    .unwrap()
                    .is_empty(),
                "empreintes identiques : aucun tirage ne doit partir"
            );
        }

        // Un message de plus d'un côté, et le désaccord réapparaît.
        ecrit(&bureau, pair, pair, 2, 11, 1_100);
        let mut tirages = 0usize;
        for offre in bureau.node.self_sync_offers().unwrap() {
            tirages += portable
                .node
                .ingest_core(&bureau.key(), offre)
                .unwrap()
                .len();
        }
        assert_eq!(tirages, 1);
    }
}
