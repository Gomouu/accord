//! Moteur des groupes (SPEC §6) : op-log signé répliqué, état déterministe,
//! synchronisation anti-entropie et rotation des clés d'epoch.
//!
//! Toute op est validée deux fois : à l'écriture locale ([`author_op`] refuse
//! d'émettre une op que l'état courant rejetterait) et à l'ingestion
//! ([`ingest_op`] vérifie la signature Ed25519 avant insertion). Les ops non
//! autorisées restent dans le log mais sont ignorées au repli — tous les pairs
//! convergent vers le même état quel que soit l'ordre d'arrivée.

pub mod crypt;
pub mod invite;
pub mod msg;
pub mod state;

pub use crypt::SEALED_KEY_LEN;
pub use msg::{
    check_thread_create_slowmode, compose_group_delete, compose_group_edit, compose_group_message,
    compose_group_poll, compose_group_reaction, compose_group_sticker, compose_group_typing,
    ingest_group_message, GroupMsgEvent,
};
pub use state::{Applied, GroupState, DEFAULT_MEMBER_PERMS};

use accord_crypto::{node_id_of, verify_signature, Identity};
use accord_proto::core_msg::{GroupOp, GroupOpBody};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

use crate::db::Db;
use crate::error::CoreError;

/// Résultat de la création d'un groupe.
#[derive(Debug)]
pub struct CreatedGroup {
    /// Identifiant du nouveau groupe.
    pub group_id: [u8; 16],
    /// Op CREATE signée, à diffuser aux futurs membres.
    pub op: GroupOp,
    /// Epoch de la clé initiale (toujours 1).
    pub key_epoch: u32,
}

/// Issue de l'ingestion d'une op distante.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestOutcome {
    /// Op nouvelle, insérée dans le log.
    Inserted,
    /// Op déjà connue (idempotence).
    Duplicate,
}

/// Offre de synchronisation (champs de `CoreMsg::GroupSync`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncOffer {
    /// Groupe concerné.
    pub group_id: [u8; 16],
    /// Lamport maximal du log local.
    pub max_lamport: u64,
    /// Nombre d'ops du log local.
    pub op_count: u64,
    /// Empreinte du log dans l'ordre total (SPEC §6.3).
    pub digest: [u8; 32],
}

/// Résultat d'une rotation de clé.
#[derive(Debug)]
pub struct KeyRotation {
    /// Nouvel epoch.
    pub key_epoch: u32,
    /// Clé scellée par membre actif : `(clé publique, clé scellée)`.
    pub sealed: Vec<([u8; 32], [u8; SEALED_KEY_LEN])>,
}

/// Génère un identifiant aléatoire de 16 octets (groupes, salons, ops).
pub fn new_id16() -> [u8; 16] {
    let mut id = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut id);
    id
}

/// État matérialisé d'un groupe depuis le log persistant.
pub fn group_state(db: &Db, group_id: &[u8; 16]) -> Result<GroupState, CoreError> {
    let ops = db.group_ops(group_id)?;
    if ops.is_empty() {
        return Err(CoreError::NotFound("groupe inconnu"));
    }
    Ok(GroupState::fold(&ops))
}

/// Crée un groupe : op CREATE signée + clé de groupe epoch 1 persistée.
pub fn create_group(
    db: &Db,
    identity: &Identity,
    name: &str,
    now_ms: u64,
) -> Result<CreatedGroup, CoreError> {
    if name.is_empty() || name.len() > 100 {
        return Err(CoreError::Invalid("nom de groupe vide ou trop long"));
    }
    let group_id = new_id16();
    let body = GroupOpBody::Create {
        name: name.to_string(),
    };
    let op = sign_op(db, identity, &group_id, &body, now_ms)?;
    db.insert_group_op(&op)?;
    db.put_group_key(&group_id, 1, &crypt::generate_group_key())?;
    Ok(CreatedGroup {
        group_id,
        op,
        key_epoch: 1,
    })
}

/// Compose, valide et persiste une op locale. Rend l'op signée à diffuser.
///
/// La validation rejoue l'op sur l'état courant : une op que les autres pairs
/// ignoreraient n'est jamais émise ([`CoreError::OpRejected`]).
pub fn author_op(
    db: &Db,
    identity: &Identity,
    group_id: &[u8; 16],
    body: &GroupOpBody,
    now_ms: u64,
) -> Result<GroupOp, CoreError> {
    let mut state = group_state(db, group_id)?;
    let op = sign_op(db, identity, group_id, body, now_ms)?;
    match state.apply(&op) {
        Applied::Ok => {
            db.insert_group_op(&op)?;
            apply_moderation(db, group_id, &state)?;
            Ok(op)
        }
        Applied::Ignored(reason) => Err(CoreError::OpRejected(reason)),
    }
}

/// `op_id` canonique d'une opération : les 16 premiers octets de
/// SHA-256(contenu) (contenu = tout sauf `op_id`/`sig`). Lier l'`op_id` au
/// contenu rend infaisable, pour un membre malveillant, de signer deux
/// opérations de contenu DIFFÉRENT partageant un même `op_id` : l'insertion
/// étant idempotente sur `op_id` et l'anti-entropie hachant les `op_id`, une
/// telle collision provoquerait une divergence d'état permanente et non
/// détectée. Avec cet invariant, deux corps distincts ont deux `op_id`
/// distincts, se répliquent tous deux et convergent par LWW.
///
/// NOTE sécurité : l'id fait 128 bits (format filaire existant), donc la
/// résistance à la collision est ≈ 2^64 (borne des anniversaires) — largement
/// suffisant pour le modèle de menace (cercle d'amis), pas « impossible » au
/// sens strict. Si le format filaire est rompu de toute façon (voir la
/// migration requise, DECISIONS/THREAT-MODEL), envisager 32 octets (≈ 2^128).
fn op_content_id(op: &GroupOp) -> [u8; 16] {
    let digest = Sha256::digest(op.content_bytes());
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest[..16]);
    id
}

/// Vrai si le groupe est « hérité » (créé avant l'`op_id` contenu-adressé) :
/// son op CREATE canonique — la première au sens de [`sort_canonical`], celle
/// que [`GroupState::fold`] applique — porte un `op_id` aléatoire d'époque.
/// Un tel groupe est *grandfathered* : ses ops à `op_id` libre restent
/// acceptées (sinon toute jonction, restauration ou anti-entropie d'un groupe
/// pré-1.3 casserait). Il conserve donc la faiblesse historique de collision
/// d'`op_id` — inchangée par rapport aux versions antérieures — tandis que
/// tout groupe créé depuis bénéficie de l'invariant fort. Sans CREATE local,
/// le régime strict s'applique — et [`should_pull`] force alors un pull
/// complet depuis 0, garantissant que le CREATE finit par arriver (en tête de
/// l'ordre canonique) et que les ops refusées sont représentées ensuite.
fn group_is_legacy(db: &Db, group_id: &[u8; 16]) -> Result<bool, CoreError> {
    let mut ops = db.group_ops(group_id)?;
    sort_canonical(&mut ops);
    Ok(ops
        .iter()
        .find(|o| o.kind == GroupOpBody::CREATE_KIND)
        .map(|create| create.op_id != op_content_id(create))
        .unwrap_or(false))
}

/// Ingère une op reçue du réseau : vérification de signature, cohérence de
/// l'`op_id` avec le contenu, insertion idempotente, avance de l'horloge de
/// Lamport locale et application des suppressions de modération à l'historique.
pub fn ingest_op(db: &Db, op: &GroupOp) -> Result<IngestOutcome, CoreError> {
    verify_signature(&op.author, &op.signable_bytes(), &op.sig)?;
    // Intégrité (SPEC §6.2) : l'`op_id` DOIT être le hash du contenu — sauf
    // régime hérité. Une op CREATE à id libre est toujours ingérable (c'est
    // elle qui établit le régime du groupe ; la refuser rendrait tout groupe
    // pré-1.3 injoignable). L'accepter n'ouvre rien de neuf : une CREATE
    // concurrente injectée par un membre malveillant — héritée OU
    // contenu-adressée, l'invariant ne discrimine pas — est ignorée au repli
    // (« groupe déjà créé ») à moins de précéder la vraie racine dans l'ordre
    // canonique, vecteur préexistant et orthogonal à cet invariant.
    if op.op_id != op_content_id(op)
        && op.kind != GroupOpBody::CREATE_KIND
        && !group_is_legacy(db, &op.group_id)?
    {
        return Err(CoreError::Invalid("op_id incohérent avec le contenu"));
    }
    // Le corps doit être décodable : une op indéchiffrable ne sert à rien
    // et polluerait le log répliqué.
    GroupOpBody::decode_body(op.kind, &op.body)?;
    db.bump_lamport(op.lamport)?;
    if !db.insert_group_op(op)? {
        return Ok(IngestOutcome::Duplicate);
    }
    let state = group_state(db, &op.group_id)?;
    apply_moderation(db, &op.group_id, &state)?;
    Ok(IngestOutcome::Inserted)
}

/// Offre de synchronisation anti-entropie pour un groupe (SPEC §6.3).
pub fn sync_offer(db: &Db, group_id: &[u8; 16]) -> Result<SyncOffer, CoreError> {
    let mut ops = db.group_ops(group_id)?;
    sort_canonical(&mut ops);
    let mut hasher = Sha256::new();
    for op in &ops {
        hasher.update(op.op_id);
    }
    let max_lamport = ops.iter().map(|o| o.lamport).max().unwrap_or(0);
    Ok(SyncOffer {
        group_id: *group_id,
        max_lamport,
        op_count: ops.len() as u64,
        digest: hasher.finalize().into(),
    })
}

/// Décide de la borne d'un `GroupSyncPull` après réception d'une offre.
///
/// - Offre identique au log local → `None` (rien à faire).
/// - Log local sans op CREATE → pull complet depuis 0 : la racine du log a pu
///   être perdue en transit (poussée d'invitation en UDP sans retransmission)
///   alors qu'une op postérieure a déjà avancé notre lamport maximal. Un pull
///   borné à `> max` ne re-livrerait JAMAIS le CREATE (famine permanente) et,
///   sans lui, le régime hérité d'un groupe pré-1.3 est indéterminable —
///   toutes ses ops à `op_id` libre seraient refusées à chaque tour.
/// - Offre plus avancée → pull depuis notre lamport maximal.
/// - Divergence à hauteur égale (empreintes différentes) → pull complet
///   depuis 0 ; l'insertion étant idempotente, seul le manquant est ajouté.
pub fn should_pull(db: &Db, offer: &SyncOffer) -> Result<Option<u64>, CoreError> {
    let local = sync_offer(db, &offer.group_id)?;
    if local.digest == offer.digest && local.op_count == offer.op_count {
        return Ok(None);
    }
    let has_create = db
        .group_ops(&offer.group_id)?
        .iter()
        .any(|o| o.kind == GroupOpBody::CREATE_KIND);
    if !has_create {
        return Ok(Some(0));
    }
    if offer.max_lamport > local.max_lamport {
        return Ok(Some(local.max_lamport));
    }
    Ok(Some(0))
}

/// Ops à renvoyer en réponse à un `GroupSyncPull { since_lamport }`.
pub fn ops_for_pull(
    db: &Db,
    group_id: &[u8; 16],
    since_lamport: u64,
) -> Result<Vec<GroupOp>, CoreError> {
    let mut ops = db.group_ops_after(group_id, since_lamport)?;
    sort_canonical(&mut ops);
    Ok(ops)
}

/// Vrai si l'identité locale est le membre responsable de la prochaine
/// rotation de clé (règle déterministe, SPEC §6.4).
pub fn is_rotation_responsible(state: &GroupState, identity: &Identity) -> bool {
    state.rotation_responsible() == Some(identity.public_key())
}

/// Effectue une rotation de clé : nouvel epoch persisté, clé scellée pour
/// chaque membre actif. Refuse si l'identité locale n'est pas responsable
/// (un pair honnête n'usurpe pas la rotation ; un pair malveillant ne peut
/// pas distribuer de clé aux autres sans qu'ils la re-scellent eux-mêmes).
pub fn rotate_key(
    db: &Db,
    identity: &Identity,
    group_id: &[u8; 16],
) -> Result<KeyRotation, CoreError> {
    let state = group_state(db, group_id)?;
    if !is_rotation_responsible(&state, identity) {
        return Err(CoreError::OpRejected(
            "non responsable de la rotation de clé",
        ));
    }
    let next_epoch = db
        .latest_group_key(group_id)?
        .map(|k| k.key_epoch + 1)
        .unwrap_or(1);
    let key = crypt::generate_group_key();
    db.put_group_key(group_id, next_epoch, &key)?;
    let mut sealed = Vec::with_capacity(state.members.len());
    for member_pk in state.members.keys() {
        sealed.push((*member_pk, crypt::seal_group_key(member_pk, &key)?));
    }
    Ok(KeyRotation {
        key_epoch: next_epoch,
        sealed,
    })
}

/// Scelle la clé courante pour un membre donné (nouvel arrivant). Seul un
/// membre actif du groupe peut être destinataire.
pub fn seal_current_key_for(
    db: &Db,
    group_id: &[u8; 16],
    member_pk: &[u8; 32],
) -> Result<(u32, [u8; SEALED_KEY_LEN]), CoreError> {
    let state = group_state(db, group_id)?;
    if !state.is_member(member_pk) {
        return Err(CoreError::OpRejected("destinataire non membre"));
    }
    let stored = db
        .latest_group_key(group_id)?
        .ok_or(CoreError::NotFound("aucune clé de groupe locale"))?;
    Ok((
        stored.key_epoch,
        crypt::seal_group_key(member_pk, &stored.key)?,
    ))
}

/// Enregistre une clé de groupe reçue (`CoreMsg::GroupKey`) après ouverture.
pub fn accept_sealed_key(
    db: &Db,
    identity: &Identity,
    group_id: &[u8; 16],
    key_epoch: u32,
    sealed_key: &[u8; SEALED_KEY_LEN],
) -> Result<(), CoreError> {
    let key = crypt::open_group_key(identity, sealed_key)?;
    db.put_group_key(group_id, key_epoch, &key)
}

/// Ordre total canonique du log (SPEC §6.2).
fn sort_canonical(ops: &mut [GroupOp]) {
    ops.sort_by_key(|op| (op.lamport, node_id_of(&op.author), op.op_id));
}

/// Construit et signe une op locale avec la prochaine horloge de Lamport.
fn sign_op(
    db: &Db,
    identity: &Identity,
    group_id: &[u8; 16],
    body: &GroupOpBody,
    now_ms: u64,
) -> Result<GroupOp, CoreError> {
    let mut op = GroupOp {
        op_id: [0u8; 16], // provisoire : remplacé par le hash du contenu
        group_id: *group_id,
        lamport: db.bump_lamport(0)?,
        wall_ms: now_ms,
        author: identity.public_key(),
        kind: body.kind(),
        body: body.encode_body(),
        sig: [0u8; 64],
    };
    // `op_id` = hash du contenu (déterministe, non falsifiable) : voir
    // [`op_content_id`]. La signature couvre ensuite cet `op_id`.
    op.op_id = op_content_id(&op);
    op.sig = identity.sign(&op.signable_bytes());
    Ok(op)
}

/// Applique les tombstones de modération du log à l'historique local, et
/// réélague le suivi local du mode lent (salon supprimé ou auteur n'étant
/// plus membre — voir [`crate::db::Db::prune_slowmode`], ce suivi vit hors
/// de `GroupState` puisqu'il n'est pas dérivable du seul op-log).
fn apply_moderation(db: &Db, group_id: &[u8; 16], state: &GroupState) -> Result<(), CoreError> {
    for msg_id in &state.moderated_deletions {
        db.delete_group_msg(msg_id, None)?;
    }
    let valid_channels: BTreeSet<[u8; 16]> = state.channels.keys().copied().collect();
    let valid_authors: BTreeSet<[u8; 32]> = state.members.keys().copied().collect();
    db.prune_slowmode(group_id, &valid_channels, &valid_authors)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use accord_proto::core_msg::ChannelKind;

    fn setup() -> (Db, Identity) {
        (
            Db::open_in_memory(&[7u8; 32]).unwrap(),
            Identity::generate_with_pow_bits(1),
        )
    }

    #[test]
    fn create_then_author_ops_builds_consistent_state() {
        let (db, id) = setup();
        let created = create_group(&db, &id, "Ma Guilde", 1_000).unwrap();
        let chan = new_id16();
        author_op(
            &db,
            &id,
            &created.group_id,
            &GroupOpBody::AddChannel {
                channel_id: chan,
                name: "général".into(),
                category: None,
                kind: ChannelKind::Text,
                position: 0,
            },
            1_001,
        )
        .unwrap();
        let state = group_state(&db, &created.group_id).unwrap();
        assert_eq!(state.name, "Ma Guilde");
        assert!(state.channels.contains_key(&chan));
        assert_eq!(state.founder, Some(id.public_key()));
        assert_eq!(
            db.latest_group_key(&created.group_id)
                .unwrap()
                .unwrap()
                .key_epoch,
            1
        );
    }

    #[test]
    fn author_op_refuses_unauthorized_action() {
        let (db, founder) = setup();
        let outsider = Identity::generate_with_pow_bits(1);
        let created = create_group(&db, &founder, "G", 0).unwrap();
        // L'étranger n'est pas membre : son op doit être refusée localement.
        let err = author_op(
            &db,
            &outsider,
            &created.group_id,
            &GroupOpBody::SetMeta {
                name: "piraté".into(),
                icon: None,
                banner_color: None,
            },
            1,
        );
        assert!(matches!(err, Err(CoreError::OpRejected(_))));
        assert_eq!(group_state(&db, &created.group_id).unwrap().name, "G");
    }

    #[test]
    fn ingest_verifies_signature_and_is_idempotent() {
        let (db_a, alice) = setup();
        let created = create_group(&db_a, &alice, "G", 0).unwrap();

        let db_b = Db::open_in_memory(&[8u8; 32]).unwrap();
        assert_eq!(
            ingest_op(&db_b, &created.op).unwrap(),
            IngestOutcome::Inserted
        );
        assert_eq!(
            ingest_op(&db_b, &created.op).unwrap(),
            IngestOutcome::Duplicate
        );

        // Signature altérée → rejet, rien d'inséré.
        let mut forged = created.op.clone();
        forged.op_id = new_id16();
        assert!(ingest_op(&db_b, &forged).is_err());
        assert_eq!(db_b.group_ops(&created.group_id).unwrap().len(), 1);
    }

    /// Fabrique une op signée à `op_id` LIBRE (non contenu-adressé), comme
    /// l'émettait un client pré-1.3 — ou comme la forgerait un membre
    /// malveillant cherchant une collision d'`op_id`.
    fn craft_free_id_op(
        identity: &Identity,
        group_id: &[u8; 16],
        body: &GroupOpBody,
        lamport: u64,
        op_id: [u8; 16],
    ) -> GroupOp {
        let mut op = GroupOp {
            op_id,
            group_id: *group_id,
            lamport,
            wall_ms: 0,
            author: identity.public_key(),
            kind: body.kind(),
            body: body.encode_body(),
            sig: [0u8; 64],
        };
        op.sig = identity.sign(&op.signable_bytes());
        op
    }

    #[test]
    fn op_id_lie_au_contenu_bloque_la_collision() {
        let (db, id) = setup();
        let created = create_group(&db, &id, "G", 0).unwrap();
        // `op_id` produit = hash du contenu.
        assert_eq!(created.op.op_id, op_content_id(&created.op));
        // Deux ops de CONTENU différent ont des `op_id` distincts : un membre
        // malveillant ne peut PAS leur donner le même `op_id` (base de la
        // divergence d'état) — chacun a un id imposé par son contenu.
        let mut a = created.op.clone();
        a.lamport = 10;
        a.op_id = op_content_id(&a);
        let mut b = created.op.clone();
        b.lamport = 11;
        b.op_id = op_content_id(&b);
        assert_ne!(a.op_id, b.op_id);
        // Dans un groupe contenu-adressé, une op (non CREATE) à `op_id`
        // falsifié, MÊME signée valablement par l'auteur, est rejetée.
        let forged = craft_free_id_op(
            &id,
            &created.group_id,
            &GroupOpBody::SetMeta {
                name: "détourné".into(),
                icon: None,
                banner_color: None,
            },
            10,
            [0xEE; 16],
        );
        assert!(ingest_op(&db, &forged).is_err());
        assert_eq!(group_state(&db, &created.group_id).unwrap().name, "G");
    }

    #[test]
    fn groupe_herite_grandfathere_accepte_ops_a_id_libre() {
        let (db_a, alice) = setup();
        let group_id = new_id16();
        // CREATE d'époque (client pré-1.3) : `op_id` aléatoire.
        let create = craft_free_id_op(
            &alice,
            &group_id,
            &GroupOpBody::Create {
                name: "Vieux".into(),
            },
            1,
            new_id16(),
        );
        assert_eq!(ingest_op(&db_a, &create).unwrap(), IngestOutcome::Inserted);
        // Le groupe est en régime hérité : les ops à id libre passent encore
        // (jonction, restauration et rattrapage des groupes pré-1.3 intacts).
        let legacy = craft_free_id_op(
            &alice,
            &group_id,
            &GroupOpBody::SetMeta {
                name: "Vieux 2".into(),
                icon: None,
                banner_color: None,
            },
            2,
            new_id16(),
        );
        assert_eq!(ingest_op(&db_a, &legacy).unwrap(), IngestOutcome::Inserted);
        // Un client à jour écrivant dans ce même groupe (id contenu-adressé)
        // passe aussi : les régimes cohabitent dans un groupe hérité.
        let mut modern = craft_free_id_op(
            &alice,
            &group_id,
            &GroupOpBody::SetMeta {
                name: "Vieux 3".into(),
                icon: None,
                banner_color: None,
            },
            3,
            [0u8; 16],
        );
        modern.op_id = op_content_id(&modern);
        modern.sig = alice.sign(&modern.signable_bytes());
        assert_eq!(ingest_op(&db_a, &modern).unwrap(), IngestOutcome::Inserted);
        assert_eq!(group_state(&db_a, &group_id).unwrap().name, "Vieux 3");
    }

    #[test]
    fn op_heritee_refusee_avant_le_create_puis_acceptee_apres() {
        let (db, alice) = setup();
        let group_id = new_id16();
        let create = craft_free_id_op(
            &alice,
            &group_id,
            &GroupOpBody::Create {
                name: "Vieux".into(),
            },
            1,
            new_id16(),
        );
        let meta = craft_free_id_op(
            &alice,
            &group_id,
            &GroupOpBody::SetMeta {
                name: "Renommé".into(),
                icon: None,
                banner_color: None,
            },
            2,
            new_id16(),
        );
        // Sans CREATE local, impossible d'établir le régime hérité : refus
        // strict. L'anti-entropie livre le CREATE en premier (ordre canonique)
        // et représentera l'op au tour suivant — convergence préservée.
        assert!(ingest_op(&db, &meta).is_err());
        ingest_op(&db, &create).unwrap();
        assert_eq!(ingest_op(&db, &meta).unwrap(), IngestOutcome::Inserted);
        assert_eq!(group_state(&db, &group_id).unwrap().name, "Renommé");
    }

    #[test]
    fn pull_repart_de_zero_quand_le_create_manque_au_log_local() {
        // Réplique complète d'un groupe hérité chez A : CREATE(1) et une op
        // d'époque (2), puis une op contenu-adressée d'un membre à jour (3)
        // et une nouvelle op d'époque (4).
        let (db_a, alice) = setup();
        let group_id = new_id16();
        let meta = |name: &str| GroupOpBody::SetMeta {
            name: name.into(),
            icon: None,
            banner_color: None,
        };
        let create = craft_free_id_op(
            &alice,
            &group_id,
            &GroupOpBody::Create {
                name: "Vieux".into(),
            },
            1,
            new_id16(),
        );
        let old2 = craft_free_id_op(&alice, &group_id, &meta("v2"), 2, new_id16());
        let mut modern = craft_free_id_op(&alice, &group_id, &meta("v3"), 3, [0u8; 16]);
        modern.op_id = op_content_id(&modern);
        modern.sig = alice.sign(&modern.signable_bytes());
        let old4 = craft_free_id_op(&alice, &group_id, &meta("v4"), 4, new_id16());
        for op in [&create, &old2, &modern, &old4] {
            ingest_op(&db_a, op).unwrap();
        }

        // B (invité fraîchement accepté) n'a reçu QUE l'op contenu-adressée :
        // la poussée du log (CREATE en tête) s'est perdue en transit. Son
        // lamport maximal (3) dépasse celui du CREATE — un pull borné à
        // `> max` ne re-livrerait jamais la racine, et chaque op d'époque
        // resterait refusée (régime indéterminable) : famine permanente.
        let db_b = Db::open_in_memory(&[4u8; 32]).unwrap();
        ingest_op(&db_b, &modern).unwrap();

        let offer_a = sync_offer(&db_a, &group_id).unwrap();
        assert_eq!(should_pull(&db_b, &offer_a).unwrap(), Some(0));

        // Le pull complet livre le log dans l'ordre canonique : le CREATE
        // établit le régime hérité, puis tout est accepté et B converge.
        for op in ops_for_pull(&db_a, &group_id, 0).unwrap() {
            if op.op_id != modern.op_id {
                ingest_op(&db_b, &op).unwrap();
            }
        }
        assert_eq!(sync_offer(&db_b, &group_id).unwrap().digest, offer_a.digest);
        assert_eq!(group_state(&db_b, &group_id).unwrap().name, "v4");
    }

    #[test]
    fn create_concurrente_ne_bascule_pas_un_groupe_moderne_en_regime_herite() {
        let (db, alice) = setup();
        let mallory = Identity::generate_with_pow_bits(1);
        let created = create_group(&db, &alice, "G", 0).unwrap();
        // Une CREATE concurrente à id libre reste ingérable (elle pourrait
        // fonder un groupe hérité légitime) mais, arrivant APRÈS la racine
        // dans l'ordre canonique, elle est ignorée au repli et ne change pas
        // le régime : le groupe reste contenu-adressé.
        let rogue_create = craft_free_id_op(
            &mallory,
            &created.group_id,
            &GroupOpBody::Create {
                name: "usurpé".into(),
            },
            created.op.lamport + 1,
            new_id16(),
        );
        assert_eq!(
            ingest_op(&db, &rogue_create).unwrap(),
            IngestOutcome::Inserted
        );
        let state = group_state(&db, &created.group_id).unwrap();
        assert_eq!(state.founder, Some(alice.public_key()));
        assert_eq!(state.name, "G");
        // Le régime strict tient toujours : op à id libre rejetée.
        let legacy = craft_free_id_op(
            &alice,
            &created.group_id,
            &GroupOpBody::SetMeta {
                name: "détourné".into(),
                icon: None,
                banner_color: None,
            },
            created.op.lamport + 2,
            new_id16(),
        );
        assert!(ingest_op(&db, &legacy).is_err());
    }

    #[test]
    fn ingested_lamport_advances_local_clock() {
        let (db_a, alice) = setup();
        let created = create_group(&db_a, &alice, "G", 0).unwrap();
        let mut op = created.op.clone();
        // Simule un pair très en avance : `op_id` recalculé sur le contenu
        // modifié (invariant op_id = hash du contenu), puis re-signé.
        op.lamport = 5_000;
        op.op_id = op_content_id(&op);
        op.sig = alice.sign(&op.signable_bytes());

        let db_b = Db::open_in_memory(&[9u8; 32]).unwrap();
        ingest_op(&db_b, &created.op).unwrap();
        ingest_op(&db_b, &op).unwrap();
        assert!(db_b.lamport().unwrap() > 5_000);
    }

    #[test]
    fn sync_offer_matches_between_replicas_and_detects_divergence() {
        let (db_a, alice) = setup();
        let created = create_group(&db_a, &alice, "G", 0).unwrap();
        let op2 = author_op(
            &db_a,
            &alice,
            &created.group_id,
            &GroupOpBody::SetMeta {
                name: "G2".into(),
                icon: None,
                banner_color: None,
            },
            1,
        )
        .unwrap();

        // Réplique complète : mêmes empreintes, aucun pull.
        let db_b = Db::open_in_memory(&[1u8; 32]).unwrap();
        ingest_op(&db_b, &created.op).unwrap();
        ingest_op(&db_b, &op2).unwrap();
        let offer_a = sync_offer(&db_a, &created.group_id).unwrap();
        let offer_b = sync_offer(&db_b, &created.group_id).unwrap();
        assert_eq!(offer_a.digest, offer_b.digest);
        assert_eq!(should_pull(&db_b, &offer_a).unwrap(), None);

        // Réplique en retard : pull depuis son lamport maximal.
        let db_c = Db::open_in_memory(&[2u8; 32]).unwrap();
        ingest_op(&db_c, &created.op).unwrap();
        let since = should_pull(&db_c, &offer_a).unwrap().unwrap();
        let missing = ops_for_pull(&db_a, &created.group_id, since).unwrap();
        assert!(missing.iter().any(|o| o.op_id == op2.op_id));
        for op in &missing {
            ingest_op(&db_c, op).unwrap();
        }
        assert_eq!(
            sync_offer(&db_c, &created.group_id).unwrap().digest,
            offer_a.digest
        );
    }

    #[test]
    fn rotation_restricted_to_responsible_and_seals_for_all_members() {
        let (db, founder) = setup();
        let bob = Identity::generate_with_pow_bits(1);
        let created = create_group(&db, &founder, "G", 0).unwrap();
        author_op(
            &db,
            &founder,
            &created.group_id,
            &GroupOpBody::AddMember {
                member: bob.public_key(),
                invite_id: None,
            },
            1,
        )
        .unwrap();

        // Bob (simple membre) n'est pas responsable.
        let err = rotate_key(&db, &bob, &created.group_id);
        assert!(matches!(err, Err(CoreError::OpRejected(_))));

        let rotation = rotate_key(&db, &founder, &created.group_id).unwrap();
        assert_eq!(rotation.key_epoch, 2);
        assert_eq!(rotation.sealed.len(), 2);
        // Bob peut ouvrir sa part et retrouver la clé persistée côté émetteur.
        let (_, sealed_for_bob) = rotation
            .sealed
            .iter()
            .find(|(pk, _)| *pk == bob.public_key())
            .unwrap();
        let opened = crypt::open_group_key(&bob, sealed_for_bob).unwrap();
        assert_eq!(opened, db.group_key(&created.group_id, 2).unwrap().unwrap());
    }

    #[test]
    fn new_member_receives_sealed_current_key() {
        let (db, founder) = setup();
        let bob = Identity::generate_with_pow_bits(1);
        let created = create_group(&db, &founder, "G", 0).unwrap();
        // Non-membre : refus.
        assert!(seal_current_key_for(&db, &created.group_id, &bob.public_key()).is_err());
        author_op(
            &db,
            &founder,
            &created.group_id,
            &GroupOpBody::AddMember {
                member: bob.public_key(),
                invite_id: None,
            },
            1,
        )
        .unwrap();
        let (epoch, sealed) =
            seal_current_key_for(&db, &created.group_id, &bob.public_key()).unwrap();

        // Côté Bob : acceptation et persistance.
        let db_bob = Db::open_in_memory(&[3u8; 32]).unwrap();
        accept_sealed_key(&db_bob, &bob, &created.group_id, epoch, &sealed).unwrap();
        assert_eq!(
            db_bob.group_key(&created.group_id, epoch).unwrap().unwrap(),
            db.group_key(&created.group_id, epoch).unwrap().unwrap()
        );
    }

    #[test]
    fn moderation_tombstone_deletes_local_history() {
        let (db, founder) = setup();
        let created = create_group(&db, &founder, "G", 0).unwrap();
        let chan = new_id16();
        author_op(
            &db,
            &founder,
            &created.group_id,
            &GroupOpBody::AddChannel {
                channel_id: chan,
                name: "gén".into(),
                category: None,
                kind: ChannelKind::Text,
                position: 0,
            },
            1,
        )
        .unwrap();
        // Un message local, puis une op de modération qui le supprime.
        let msg_id = new_id16();
        let rec = crate::db::GroupMsgRecord {
            msg_id,
            group_id: created.group_id,
            channel_id: chan,
            author: founder.public_key(),
            lamport: 10,
            sent_ms: 10,
            kind: 0,
            body: b"a moderer".to_vec(),
            deleted: false,
            edited: None,
        };
        db.insert_group_msg(&rec).unwrap();
        author_op(
            &db,
            &founder,
            &created.group_id,
            &GroupOpBody::DeleteMsg {
                channel_id: chan,
                msg_id,
            },
            2,
        )
        .unwrap();
        let history = db
            .group_history(&created.group_id, &chan, u64::MAX, 10)
            .unwrap();
        assert!(history.iter().all(|m| m.msg_id != msg_id || m.deleted));
    }
}
