//! Protocole d'invitation à consentement explicite (D-045).
//!
//! Remplace l'ancien force-join (`AddMember` + op-log complet + clé poussés
//! sans demande ni accord) par un aller-retour en deux temps :
//!
//! 1. L'inviteur autorise une invitation (op `InviteCreate`, déjà répliquée
//!    et vérifiée au repli, [`super::state::GroupState::apply`]) puis envoie
//!    un `CoreMsg::InviteTicket` signé à UN invité précis (transport
//!    point-à-point, jamais répliqué).
//! 2. L'invité vérifie la signature du ticket, le stocke localement en
//!    attente, et — seulement si l'utilisateur accepte explicitement — répond
//!    par `CoreMsg::InviteAccept` prouvant qu'il détient le secret. L'inviteur
//!    ré-vérifie alors l'invitation (droits, expiration, secret) avant
//!    d'admettre le membre et de lui pousser l'op-log et la clé de groupe.
//!
//! La porte de consentement côté invité (matérialisation locale non
//! répliquée, [`crate::db::LocalMembership`]) est appliquée par l'appelant
//! réseau (`accord-node`) ; ce module ne fait que fournir les primitives
//! cryptographiques et l'autorité de décision côté inviteur.

use accord_crypto::{verify_signature, Identity};
use accord_proto::core_msg::{invite_ticket_signable_bytes, perms, CoreMsg, GroupOp, GroupOpBody};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::db::Db;
use crate::error::CoreError;

use super::crypt::SEALED_KEY_LEN;
use super::{author_op, group_state, new_id16, ops_for_pull, seal_current_key_for};

/// Durée de vie minimale d'un ticket d'invitation (1 minute) : borne basse
/// déterministe, évite une expiration immédiate par erreur d'appel.
pub const MIN_INVITE_TTL_MS: u64 = 60_000;
/// Durée de vie maximale d'un ticket d'invitation (30 jours).
pub const MAX_INVITE_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1000;
/// Durée de vie par défaut d'un ticket d'invitation (7 jours), utilisée par
/// la frontière API quand l'appelant n'en précise pas.
pub const DEFAULT_INVITE_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// Invitation à usage unique fraîchement autorisée : op signée à répliquer,
/// identifiant, secret en clair (à transporter via [`build_invite_ticket`],
/// jamais persisté tel quel) et expiration effective.
#[derive(Debug)]
pub struct AuthoredInvite {
    /// Op `InviteCreate` signée, à diffuser aux membres actuels.
    pub op: GroupOp,
    /// Identifiant de l'invitation.
    pub invite_id: [u8; 16],
    /// Secret en clair (32 octets aléatoires) : préimage de `code_hash`.
    pub secret: [u8; 32],
    /// Expiration murale ms effective (0 = n'expire jamais, uniquement via
    /// [`author_invite_create_with`] ; toujours bornée sinon).
    pub expires_ms: u64,
}

/// Génère un secret d'invitation aléatoire et son empreinte SHA-256.
fn new_invite_secret() -> ([u8; 32], [u8; 32]) {
    let mut secret = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut secret);
    let hash: [u8; 32] = Sha256::digest(secret).into();
    (secret, hash)
}

/// Autorise une invitation à usage unique (op `InviteCreate`) pour `group_id`.
/// `ttl_ms` est bornée à `[MIN_INVITE_TTL_MS, MAX_INVITE_TTL_MS]`. Échoue
/// comme tout op locale si l'identité ne détient pas `INVITE`
/// ([`author_op`] rejoue l'op sur l'état courant).
pub fn author_invite_create(
    db: &Db,
    identity: &Identity,
    group_id: &[u8; 16],
    now_ms: u64,
    ttl_ms: u64,
) -> Result<AuthoredInvite, CoreError> {
    author_invite_create_with(db, identity, group_id, now_ms, 1, Some(ttl_ms))
}

/// Autorise une invitation paramétrable (op `InviteCreate`) pour `group_id` :
/// `max_uses = 0` = illimité, `ttl_ms = None` = n'expire jamais
/// (`expires_ms = 0`, forme déjà comprise par le repli
/// [`super::state::GroupState::apply`]), `Some(ttl)` borné à
/// `[MIN_INVITE_TTL_MS, MAX_INVITE_TTL_MS]` comme [`author_invite_create`].
pub fn author_invite_create_with(
    db: &Db,
    identity: &Identity,
    group_id: &[u8; 16],
    now_ms: u64,
    max_uses: u32,
    ttl_ms: Option<u64>,
) -> Result<AuthoredInvite, CoreError> {
    let expires_ms = match ttl_ms {
        None => 0,
        Some(ttl) => now_ms.saturating_add(ttl.clamp(MIN_INVITE_TTL_MS, MAX_INVITE_TTL_MS)),
    };
    let invite_id = new_id16();
    let (secret, code_hash) = new_invite_secret();
    let op = author_op(
        db,
        identity,
        group_id,
        &GroupOpBody::InviteCreate {
            invite_id,
            code_hash,
            max_uses,
            expires_ms,
        },
        now_ms,
    )?;
    Ok(AuthoredInvite {
        op,
        invite_id,
        secret,
        expires_ms,
    })
}

/// Construit et signe le ticket transportable à envoyer à l'invité
/// (`CoreMsg::InviteTicket`), point-à-point, hors op-log.
pub fn build_invite_ticket(
    identity: &Identity,
    group_id: &[u8; 16],
    invite_id: &[u8; 16],
    group_name: &str,
    secret: &[u8; 32],
    expires_ms: u64,
) -> CoreMsg {
    let inviter = identity.public_key();
    let bytes = invite_ticket_signable_bytes(
        group_id, invite_id, group_name, &inviter, secret, expires_ms,
    );
    let sig = identity.sign(&bytes);
    CoreMsg::InviteTicket {
        group_id: *group_id,
        invite_id: *invite_id,
        group_name: group_name.to_string(),
        inviter,
        secret: *secret,
        expires_ms,
        sig,
    }
}

/// Vérifie la signature d'un `InviteTicket` reçu et son expiration de
/// transport (indépendante de l'expiration de l'op `InviteCreate`,
/// revérifiée côté inviteur au moment de l'acceptation). Ne matérialise
/// jamais le groupe : le seul effet attendu est « ticket digne d'être
/// affiché à l'utilisateur ».
#[allow(clippy::too_many_arguments)]
pub fn verify_invite_ticket(
    group_id: &[u8; 16],
    invite_id: &[u8; 16],
    group_name: &str,
    inviter: &[u8; 32],
    secret: &[u8; 32],
    expires_ms: u64,
    sig: &[u8; 64],
    now_ms: u64,
) -> Result<(), CoreError> {
    if expires_ms != 0 && now_ms > expires_ms {
        return Err(CoreError::Invalid("ticket d'invitation expiré"));
    }
    let bytes =
        invite_ticket_signable_bytes(group_id, invite_id, group_name, inviter, secret, expires_ms);
    verify_signature(inviter, &bytes, sig)?;
    Ok(())
}

/// Résultat d'une acceptation finalisée côté inviteur : op `AddMember`
/// signée, journal complet à rejouer chez le nouveau membre, et clé de
/// groupe courante scellée pour lui — exactement ce que l'ancien force-join
/// poussait sans demande, désormais gated par une acceptation prouvée.
#[derive(Debug)]
pub struct FinalizedInvite {
    /// Op `AddMember` signée, à diffuser aux membres actuels.
    pub add_op: GroupOp,
    /// Journal complet du groupe (ordre canonique), à envoyer au nouveau
    /// membre pour qu'il matérialise l'état.
    pub ops: Vec<GroupOp>,
    /// Epoch de la clé de groupe scellée.
    pub key_epoch: u32,
    /// Clé de groupe courante, scellée pour le nouveau membre.
    pub sealed_key: [u8; SEALED_KEY_LEN],
}

/// Finalise une acceptation d'invitation (`CoreMsg::InviteAccept`/`InviteRedeem`)
/// côté créateur : n'agit QUE si l'identité locale est le créateur de
/// l'invitation, re-vérifie qu'elle détient toujours `INVITE`, que
/// l'invitation est encore valide (non révoquée/expirée/épuisée, selon
/// l'état replié — [`super::state::GroupState::invites`]) et que `secret`
/// correspond bien à `code_hash` (seul le détenteur du ticket/lien peut le
/// prouver), puis admet le membre.
///
/// CRITICAL/HIGH (liens publics) : un lien partagé peut être présenté par un
/// attaquant à plusieurs membres INVITE en même temps. Chaque réplique locale
/// verrait `uses < max_uses` et scellerait la clé, laissant fuiter une clé de
/// groupe valide à un invité que le repli canonique rejettera pourtant.
/// N'admettre et ne sceller que chez le créateur sérialise la consommation
/// des usages sur son unique mutex : la course inter-nœuds disparaît. Un
/// membre INVITE non créateur qui reçoit un redeem valide obtient donc un
/// résultat neutre `Ok(None)` (ni op-log, ni clé), à ne pas confondre avec
/// une entrée invalide (`Err`).
///
/// N'échoue jamais par panique sur une entrée attaquant-contrôlée : toute
/// invitation/secret invalide rend une erreur explicite, à ignorer
/// silencieusement par l'appelant réseau (pas de canal d'erreur vers un pair
/// non authentifié sur cette invitation).
pub fn finalize_invite_accept(
    db: &Db,
    identity: &Identity,
    group_id: &[u8; 16],
    invite_id: &[u8; 16],
    secret: &[u8; 32],
    invitee: &[u8; 32],
    now_ms: u64,
) -> Result<Option<FinalizedInvite>, CoreError> {
    let state = group_state(db, group_id)?;
    let invite = state
        .invites
        .get(invite_id)
        .ok_or(CoreError::NotFound("invitation inconnue"))?;
    // Seul le créateur finalise et scelle (CRITICAL/HIGH) : cas neutre, pas
    // une entrée invalide.
    if identity.public_key() != invite.creator {
        return Ok(None);
    }
    if !state.can(&identity.public_key(), perms::INVITE) {
        return Err(CoreError::OpRejected("droit d'invitation révoqué"));
    }
    if invite.revoked
        || (invite.max_uses > 0 && invite.uses >= invite.max_uses)
        || (invite.expires_ms > 0 && now_ms > invite.expires_ms)
    {
        return Err(CoreError::OpRejected(
            "invitation révoquée, expirée ou épuisée",
        ));
    }
    let hash: [u8; 32] = Sha256::digest(secret).into();
    if hash != invite.code_hash {
        return Err(CoreError::OpRejected("secret d'invitation invalide"));
    }
    let add_op = author_op(
        db,
        identity,
        group_id,
        &GroupOpBody::AddMember {
            member: *invitee,
            invite_id: Some(*invite_id),
        },
        now_ms,
    )?;
    let ops = ops_for_pull(db, group_id, 0)?;
    let (key_epoch, sealed_key) = seal_current_key_for(db, group_id, invitee)?;
    Ok(Some(FinalizedInvite {
        add_op,
        ops,
        key_epoch,
        sealed_key,
    }))
}

// ---- Liens d'invitation publics partageables ----

/// Préfixe lisible d'un lien d'invitation partageable.
pub const INVITE_LINK_PREFIX: &str = "accord://invite/";

/// Borne du nom de groupe embarqué dans un lien (octets UTF-8, tronqué à
/// l'encodage sur une frontière de caractère).
pub const MAX_LINK_NAME_BYTES: usize = 48;

/// Versions du format binaire d'un lien d'invitation. v1 = tête ‖ nom ‖
/// somme de contrôle ; v2 = v1 augmentée d'un bloc icône/bannière/couleur
/// inséré entre le nom et la somme de contrôle. Rétrocompatible : le décodage
/// discrimine le découpage du payload d'après l'octet de version.
const INVITE_LINK_VERSION_V1: u8 = 1;
const INVITE_LINK_VERSION_V2: u8 = 2;

/// Version émise à l'encodage (toujours la plus récente).
const INVITE_LINK_VERSION: u8 = INVITE_LINK_VERSION_V2;

/// Taille des champs fixes de tête du payload : version ‖ group_id ‖
/// invite_id ‖ secret ‖ inviter. Commune aux deux versions ; le nom de groupe
/// (variable) suit immédiatement.
const INVITE_LINK_FIXED_LEN: usize = 1 + 16 + 16 + 32 + 32;

/// Taille du bloc additif v2, placé APRÈS le nom variable et AVANT la somme de
/// contrôle : icon_root(32) ‖ banner_root(32) ‖ banner_color(4, u32
/// big-endian). Une racine toute à zéro (ou une couleur nulle) = « absente ».
const INVITE_LINK_V2_TAIL_LEN: usize = 32 + 32 + 4;

/// Taille de la somme de contrôle (préfixe SHA-256 du payload) : détecte un
/// code corrompu ou tronqué avant tout aller-retour réseau. Ce n'est PAS une
/// protection d'intégrité cryptographique — le secret n'est validé que côté
/// inviteur (hash contre l'op répliquée).
const INVITE_LINK_CHECKSUM_LEN: usize = 4;

/// Plafond de taille du texte encodé (base64url) accepté en entrée : une
/// chaîne démesurée est rejetée AVANT tout décodage base64 (défense en
/// profondeur, borne le coût CPU de l'allocation/décodage sur une entrée
/// attaquant-contrôlée). Large devant le maximum réel (~217 octets pour un
/// lien v2 au nom le plus long) sans être exploitable.
const MAX_INVITE_LINK_ENCODED_LEN: usize = 512;

/// Contenu décodé d'un lien d'invitation partageable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteLink {
    /// Groupe à rejoindre.
    pub group_id: [u8; 16],
    /// Invitation (op `InviteCreate` répliquée chez les membres).
    pub invite_id: [u8; 16],
    /// Secret d'invitation (préimage de `code_hash`).
    pub secret: [u8; 32],
    /// Clé publique de l'inviteur à contacter pour racheter le lien.
    pub inviter: [u8; 32],
    /// Nom du groupe au moment de la création du lien (affichage avant
    /// adhésion uniquement — jamais une source d'autorité).
    pub group_name: String,
    /// Racine Merkle de l'icône du serveur (v2), `None` si absente ou lien v1.
    pub icon_root: Option<[u8; 32]>,
    /// Racine Merkle de la bannière du serveur (v2), `None` si absente ou v1.
    pub banner_root: Option<[u8; 32]>,
    /// Couleur de bannière `0xRRGGBB` (v2), `None` si nulle ou lien v1.
    pub banner_color: Option<u32>,
}

/// Alphabet base64 URL-safe (RFC 4648 §5), sans padding.
const B64URL_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encode en base64 URL-safe sans padding.
fn b64url_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let acc = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        let n_chars = chunk.len() + 1;
        for i in 0..n_chars {
            out.push(B64URL_ALPHABET[(acc >> (18 - 6 * i)) as usize & 0x3F] as char);
        }
    }
    out
}

/// Valeur d'un caractère base64 URL-safe (rejet strict hors alphabet).
fn b64url_val(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

/// Décode du base64 URL-safe sans padding : rejette tout caractère hors
/// alphabet, toute longueur impossible et tout encodage non canonique (bits
/// de bourrage non nuls) — entrée potentiellement hostile, jamais de panique.
fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    if bytes.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut acc = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= b64url_val(c)? << (18 - 6 * i);
        }
        match chunk.len() {
            4 => out.extend_from_slice(&[(acc >> 16) as u8, (acc >> 8) as u8, acc as u8]),
            3 => {
                if acc & 0xFF != 0 {
                    return None;
                }
                out.extend_from_slice(&[(acc >> 16) as u8, (acc >> 8) as u8]);
            }
            2 => {
                if acc & 0xFFFF != 0 {
                    return None;
                }
                out.push((acc >> 16) as u8);
            }
            _ => return None,
        }
    }
    Some(out)
}

/// Tronque `name` à [`MAX_LINK_NAME_BYTES`] octets UTF-8 sur une frontière de
/// caractère (jamais de panique sur un multi-octets).
fn truncate_link_name(name: &str) -> &str {
    if name.len() <= MAX_LINK_NAME_BYTES {
        return name;
    }
    let mut end = MAX_LINK_NAME_BYTES;
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name[..end]
}

/// Encode un lien d'invitation partageable v2 :
/// `accord://invite/<base64url>` où le payload est `version(=2) ‖ group_id ‖
/// invite_id ‖ secret ‖ inviter ‖ nom (≤ 48 octets) ‖ icon_root(32) ‖
/// banner_root(32) ‖ banner_color(4, u32 big-endian)` suivi d'une somme de
/// contrôle de 4 octets. Une racine `None` est émise toute à zéro et une
/// couleur `None` comme zéro (relues comme « absentes » au décodage).
#[allow(clippy::too_many_arguments)]
pub fn encode_invite_link(
    group_id: &[u8; 16],
    invite_id: &[u8; 16],
    secret: &[u8; 32],
    inviter: &[u8; 32],
    group_name: &str,
    icon_root: Option<[u8; 32]>,
    banner_root: Option<[u8; 32]>,
    banner_color: Option<u32>,
) -> String {
    let name = truncate_link_name(group_name);
    let mut payload = Vec::with_capacity(
        INVITE_LINK_FIXED_LEN + name.len() + INVITE_LINK_V2_TAIL_LEN + INVITE_LINK_CHECKSUM_LEN,
    );
    payload.push(INVITE_LINK_VERSION);
    payload.extend_from_slice(group_id);
    payload.extend_from_slice(invite_id);
    payload.extend_from_slice(secret);
    payload.extend_from_slice(inviter);
    payload.extend_from_slice(name.as_bytes());
    payload.extend_from_slice(&icon_root.unwrap_or([0u8; 32]));
    payload.extend_from_slice(&banner_root.unwrap_or([0u8; 32]));
    payload.extend_from_slice(&banner_color.unwrap_or(0).to_be_bytes());
    let checksum: [u8; 32] = Sha256::digest(&payload).into();
    payload.extend_from_slice(&checksum[..INVITE_LINK_CHECKSUM_LEN]);
    format!("{INVITE_LINK_PREFIX}{}", b64url_encode(&payload))
}

/// Décode et valide un lien d'invitation (préfixe optionnel, espaces
/// tolérés aux extrémités). Rejette tout code corrompu : base64 invalide,
/// version inconnue, longueur hors bornes, somme de contrôle fausse, nom
/// non-UTF-8 ou indigne d'affichage (caractères de contrôle/usurpateurs,
/// [`super::state::is_valid_display_label`]). Entrée non fiable : erreurs
/// explicites, jamais de panique.
pub fn decode_invite_link(code: &str) -> Result<InviteLink, CoreError> {
    let trimmed = code.trim();
    let encoded = trimmed.strip_prefix(INVITE_LINK_PREFIX).unwrap_or(trimmed);
    if encoded.len() > MAX_INVITE_LINK_ENCODED_LEN {
        return Err(CoreError::Invalid("lien d'invitation trop long"));
    }
    let bytes = b64url_decode(encoded).ok_or(CoreError::Invalid("lien d'invitation illisible"))?;
    // Au moins l'octet de version et la somme de contrôle doivent tenir avant
    // toute lecture — l'octet de version pilote le découpage du reste.
    if bytes.len() <= INVITE_LINK_CHECKSUM_LEN {
        return Err(CoreError::Invalid("lien d'invitation tronqué ou trop long"));
    }
    // Taille du bloc fixe placé après le nom variable, selon la version.
    let tail_len = match bytes[0] {
        INVITE_LINK_VERSION_V1 => 0,
        INVITE_LINK_VERSION_V2 => INVITE_LINK_V2_TAIL_LEN,
        _ => return Err(CoreError::Invalid("version de lien d'invitation inconnue")),
    };
    // Bornes de longueur totale (nom variable ≤ 48 octets) : rejette tout
    // octet en trop comme v1, jamais de tranche négative pour le nom v2.
    let min = INVITE_LINK_FIXED_LEN + tail_len + INVITE_LINK_CHECKSUM_LEN;
    if bytes.len() < min || bytes.len() > min + MAX_LINK_NAME_BYTES {
        return Err(CoreError::Invalid("lien d'invitation tronqué ou trop long"));
    }
    let (payload, checksum) = bytes.split_at(bytes.len() - INVITE_LINK_CHECKSUM_LEN);
    let expected: [u8; 32] = Sha256::digest(payload).into();
    if checksum != &expected[..INVITE_LINK_CHECKSUM_LEN] {
        return Err(CoreError::Invalid("lien d'invitation corrompu"));
    }
    // Lecture sans voie de panique (D23) : les bornes fixes sont vérifiées en
    // amont ; un dépassement impossible rend un tableau nul (traité « absent »
    // par `opt_root`) plutôt que de paniquer.
    fn arr<const N: usize>(payload: &[u8], from: usize) -> [u8; N] {
        let mut out = [0u8; N];
        if let Some(s) = payload.get(from..from + N) {
            out.copy_from_slice(s);
        }
        out
    }
    // Une racine toute à zéro (ou couleur nulle) signifie « absente ».
    fn opt_root(root: [u8; 32]) -> Option<[u8; 32]> {
        if root == [0u8; 32] {
            None
        } else {
            Some(root)
        }
    }
    // Le nom occupe l'espace entre la tête fixe et le bloc v2 (nul en v1).
    let name_end = payload.len() - tail_len;
    let group_name = core::str::from_utf8(&payload[INVITE_LINK_FIXED_LEN..name_end])
        .map_err(|_| CoreError::Invalid("nom de groupe du lien illisible"))?;
    if !super::state::is_valid_display_label(group_name, MAX_LINK_NAME_BYTES) {
        return Err(CoreError::Invalid("nom de groupe du lien invalide"));
    }
    let (icon_root, banner_root, banner_color) = if tail_len == 0 {
        (None, None, None)
    } else {
        let color = u32::from_be_bytes(arr(payload, name_end + 64));
        (
            opt_root(arr(payload, name_end)),
            opt_root(arr(payload, name_end + 32)),
            (color != 0).then_some(color),
        )
    };
    Ok(InviteLink {
        group_id: arr(payload, 1),
        invite_id: arr(payload, 17),
        secret: arr(payload, 33),
        inviter: arr(payload, 65),
        group_name: group_name.to_string(),
        icon_root,
        banner_root,
        banner_color,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group::create_group;

    fn setup() -> (Db, Identity) {
        (
            Db::open_in_memory(&[21u8; 32]).unwrap(),
            Identity::generate_with_pow_bits(1),
        )
    }

    #[test]
    fn ttl_is_clamped_and_never_zero() {
        let (db, founder) = setup();
        let created = create_group(&db, &founder, "G", 0).unwrap();
        let inv = author_invite_create(&db, &founder, &created.group_id, 1_000, 0).unwrap();
        assert_eq!(inv.expires_ms, 1_000 + MIN_INVITE_TTL_MS);

        let inv2 = author_invite_create(&db, &founder, &created.group_id, 1_000, u64::MAX).unwrap();
        assert_eq!(inv2.expires_ms, 1_000 + MAX_INVITE_TTL_MS);
    }

    #[test]
    fn ticket_roundtrips_signature_and_rejects_tampering() {
        let (db, founder) = setup();
        let created = create_group(&db, &founder, "Guilde", 0).unwrap();
        let inv = author_invite_create(&db, &founder, &created.group_id, 0, 0).unwrap();
        let ticket = build_invite_ticket(
            &founder,
            &created.group_id,
            &inv.invite_id,
            "Guilde",
            &inv.secret,
            inv.expires_ms,
        );
        let CoreMsg::InviteTicket {
            group_id,
            invite_id,
            group_name,
            inviter,
            secret,
            expires_ms,
            sig,
        } = ticket
        else {
            unreachable!("build_invite_ticket produit un InviteTicket");
        };
        verify_invite_ticket(
            &group_id,
            &invite_id,
            &group_name,
            &inviter,
            &secret,
            expires_ms,
            &sig,
            0,
        )
        .unwrap();

        // Signature forgée (secret altéré après coup) : rejetée.
        assert!(verify_invite_ticket(
            &group_id,
            &invite_id,
            &group_name,
            &inviter,
            &[0xFF; 32],
            expires_ms,
            &sig,
            0,
        )
        .is_err());

        // Ticket déjà expiré au moment de la vérification : rejeté.
        assert!(verify_invite_ticket(
            &group_id,
            &invite_id,
            &group_name,
            &inviter,
            &secret,
            1_000,
            &sig,
            2_000,
        )
        .is_err());
    }

    #[test]
    fn finalize_requires_correct_secret_and_is_single_use() {
        let (db, founder) = setup();
        let bob = Identity::generate_with_pow_bits(1);
        let created = create_group(&db, &founder, "Guilde", 0).unwrap();
        let inv = author_invite_create(&db, &founder, &created.group_id, 0, 0).unwrap();

        // Mauvais secret : refusé, personne n'est admis.
        assert!(finalize_invite_accept(
            &db,
            &founder,
            &created.group_id,
            &inv.invite_id,
            &[0xAA; 32],
            &bob.public_key(),
            1,
        )
        .is_err());
        assert!(!group_state(&db, &created.group_id)
            .unwrap()
            .is_member(&bob.public_key()));

        // Bon secret : admis, la clé lui est scellée.
        let finalized = finalize_invite_accept(
            &db,
            &founder,
            &created.group_id,
            &inv.invite_id,
            &inv.secret,
            &bob.public_key(),
            2,
        )
        .unwrap()
        .expect("le créateur finalise l'admission");
        assert!(finalized.ops.iter().any(|o| o.kind == 0x07));
        assert!(group_state(&db, &created.group_id)
            .unwrap()
            .is_member(&bob.public_key()));

        // Rejeu (invitation à usage unique déjà consommée) : refusé.
        let carol = Identity::generate_with_pow_bits(1);
        assert!(finalize_invite_accept(
            &db,
            &founder,
            &created.group_id,
            &inv.invite_id,
            &inv.secret,
            &carol.public_key(),
            3,
        )
        .is_err());
    }

    #[test]
    fn invite_link_roundtrips_with_and_without_prefix() {
        let code = encode_invite_link(
            &[1; 16], &[2; 16], &[3; 32], &[4; 32], "Guilde", None, None, None,
        );
        assert!(code.starts_with(INVITE_LINK_PREFIX));
        let link = decode_invite_link(&code).unwrap();
        assert_eq!(
            link,
            InviteLink {
                group_id: [1; 16],
                invite_id: [2; 16],
                secret: [3; 32],
                inviter: [4; 32],
                group_name: "Guilde".into(),
                icon_root: None,
                banner_root: None,
                banner_color: None,
            }
        );
        // Sans préfixe et avec espaces parasites : toujours décodable.
        let bare = code.strip_prefix(INVITE_LINK_PREFIX).unwrap();
        assert_eq!(decode_invite_link(&format!("  {bare} \n")).unwrap(), link);
    }

    #[test]
    fn invite_link_v2_roundtrips_icon_banner_color() {
        let icon = [0xA1u8; 32];
        let banner = [0xB2u8; 32];
        let code = encode_invite_link(
            &[1; 16],
            &[2; 16],
            &[3; 32],
            &[4; 32],
            "Guilde",
            Some(icon),
            Some(banner),
            Some(0x5865F2),
        );
        let link = decode_invite_link(&code).unwrap();
        assert_eq!(
            link,
            InviteLink {
                group_id: [1; 16],
                invite_id: [2; 16],
                secret: [3; 32],
                inviter: [4; 32],
                group_name: "Guilde".into(),
                icon_root: Some(icon),
                banner_root: Some(banner),
                banner_color: Some(0x5865F2),
            }
        );
    }

    #[test]
    fn invite_link_v2_zero_icon_banner_color_decode_to_none() {
        // Racines toutes à zéro et couleur nulle passées explicitement : le
        // décodage les rend « absentes » (None), pas des zéros signifiants.
        let code = encode_invite_link(
            &[1; 16],
            &[2; 16],
            &[3; 32],
            &[4; 32],
            "Guilde",
            Some([0u8; 32]),
            Some([0u8; 32]),
            Some(0),
        );
        let link = decode_invite_link(&code).unwrap();
        assert_eq!(link.icon_root, None);
        assert_eq!(link.banner_root, None);
        assert_eq!(link.banner_color, None);
        // Une seule facette renseignée : les autres restent None.
        let code2 = encode_invite_link(
            &[1; 16],
            &[2; 16],
            &[3; 32],
            &[4; 32],
            "Guilde",
            None,
            Some([7u8; 32]),
            None,
        );
        let link2 = decode_invite_link(&code2).unwrap();
        assert_eq!(link2.icon_root, None);
        assert_eq!(link2.banner_root, Some([7u8; 32]));
        assert_eq!(link2.banner_color, None);
    }

    #[test]
    fn invite_link_v1_still_decodes_with_none_metadata() {
        // Payload v1 forgé à la main (octet de version = 1, pas de bloc v2) :
        // un ancien lien reste décodable et rend des métadonnées absentes.
        let mut payload = Vec::new();
        payload.push(INVITE_LINK_VERSION_V1);
        payload.extend_from_slice(&[1u8; 16]);
        payload.extend_from_slice(&[2u8; 16]);
        payload.extend_from_slice(&[3u8; 32]);
        payload.extend_from_slice(&[4u8; 32]);
        payload.extend_from_slice(b"Guilde");
        let checksum: [u8; 32] = Sha256::digest(&payload).into();
        payload.extend_from_slice(&checksum[..INVITE_LINK_CHECKSUM_LEN]);
        let code = format!("{INVITE_LINK_PREFIX}{}", b64url_encode(&payload));
        let link = decode_invite_link(&code).unwrap();
        assert_eq!(
            link,
            InviteLink {
                group_id: [1; 16],
                invite_id: [2; 16],
                secret: [3; 32],
                inviter: [4; 32],
                group_name: "Guilde".into(),
                icon_root: None,
                banner_root: None,
                banner_color: None,
            }
        );
    }

    #[test]
    fn invite_link_truncates_name_on_char_boundary() {
        // 30 × 'é' = 60 octets UTF-8 : tronqué à ≤ 48 octets sans couper un
        // caractère (48 impair pour un alphabet 2 octets → 47 octets utiles).
        let long = "é".repeat(30);
        let code = encode_invite_link(
            &[1; 16], &[2; 16], &[3; 32], &[4; 32], &long, None, None, None,
        );
        let link = decode_invite_link(&code).unwrap();
        assert!(link.group_name.len() <= MAX_LINK_NAME_BYTES);
        assert!(link.group_name.chars().all(|c| c == 'é'));
    }

    #[test]
    fn corrupted_or_forged_invite_links_are_rejected() {
        let code = encode_invite_link(
            &[1; 16], &[2; 16], &[3; 32], &[4; 32], "Guilde", None, None, None,
        );
        // Un caractère altéré : somme de contrôle fausse.
        let mut chars: Vec<char> = code.chars().collect();
        let last = chars.len() - 1;
        chars[last] = if chars[last] == 'A' { 'B' } else { 'A' };
        let altered: String = chars.into_iter().collect();
        assert!(decode_invite_link(&altered).is_err());
        // Tronqué : rejeté (longueur ou checksum, jamais de panique).
        assert!(decode_invite_link(&code[..code.len() - 8]).is_err());
        // Base64 invalide, vide, ou déchets : rejetés.
        assert!(decode_invite_link("accord://invite/!!!!").is_err());
        assert!(decode_invite_link("").is_err());
        assert!(decode_invite_link("accord://invite/").is_err());
        // Nom usurpateur (caractère de contrôle) forgé dans un code
        // par ailleurs bien formé : rejeté au décodage.
        let forged = encode_invite_link(
            &[1; 16],
            &[2; 16],
            &[3; 32],
            &[4; 32],
            "Gui\u{7}lde",
            None,
            None,
            None,
        );
        assert!(decode_invite_link(&forged).is_err());
    }

    #[test]
    fn invite_link_v2_corrupted_checksum_is_rejected() {
        let code = encode_invite_link(
            &[1; 16],
            &[2; 16],
            &[3; 32],
            &[4; 32],
            "Guilde",
            Some([9u8; 32]),
            Some([8u8; 32]),
            Some(0x112233),
        );
        // Dernier caractère altéré : la somme de contrôle ne correspond plus.
        let mut chars: Vec<char> = code.chars().collect();
        let last = chars.len() - 1;
        chars[last] = if chars[last] == 'A' { 'B' } else { 'A' };
        let altered: String = chars.into_iter().collect();
        assert!(decode_invite_link(&altered).is_err());
    }

    #[test]
    fn invite_link_v2_truncated_tail_is_rejected() {
        // Un lien v2 amputé de quelques octets de sa queue fixe : rejeté
        // (longueur hors bornes ou somme de contrôle fausse), jamais de panique.
        let code = encode_invite_link(
            &[1; 16],
            &[2; 16],
            &[3; 32],
            &[4; 32],
            "Guilde",
            Some([9u8; 32]),
            Some([8u8; 32]),
            Some(0x445566),
        );
        let bare = code.strip_prefix(INVITE_LINK_PREFIX).unwrap();
        let raw = b64url_decode(bare).unwrap();
        // Retire les 10 derniers octets (rogne le bloc icône/bannière/somme).
        let truncated = b64url_encode(&raw[..raw.len() - 10]);
        assert!(decode_invite_link(&truncated).is_err());
    }

    #[test]
    fn invite_link_over_length_name_is_rejected() {
        // Payload v2 forgé avec un nom de 49 octets (> MAX_LINK_NAME_BYTES) :
        // la borne de longueur totale le rejette avant toute interprétation.
        let mut payload = Vec::new();
        payload.push(INVITE_LINK_VERSION_V2);
        payload.extend_from_slice(&[1u8; 16]);
        payload.extend_from_slice(&[2u8; 16]);
        payload.extend_from_slice(&[3u8; 32]);
        payload.extend_from_slice(&[4u8; 32]);
        payload.extend_from_slice(&b"a".repeat(MAX_LINK_NAME_BYTES + 1));
        payload.extend_from_slice(&[0u8; INVITE_LINK_V2_TAIL_LEN]);
        let checksum: [u8; 32] = Sha256::digest(&payload).into();
        payload.extend_from_slice(&checksum[..INVITE_LINK_CHECKSUM_LEN]);
        let code = format!("{INVITE_LINK_PREFIX}{}", b64url_encode(&payload));
        assert!(decode_invite_link(&code).is_err());
    }

    #[test]
    fn author_invite_create_with_supports_unlimited_and_never_expiring() {
        let (db, founder) = setup();
        let created = create_group(&db, &founder, "G", 0).unwrap();
        let inv =
            author_invite_create_with(&db, &founder, &created.group_id, 1_000, 0, None).unwrap();
        assert_eq!(inv.expires_ms, 0, "0 = n'expire jamais");
        let state = group_state(&db, &created.group_id).unwrap();
        let entry = state.invites.get(&inv.invite_id).unwrap();
        assert_eq!(entry.max_uses, 0, "0 = usages illimités");
        assert_eq!(entry.expires_ms, 0);
        // La forme bornée reste bornée.
        let bounded =
            author_invite_create_with(&db, &founder, &created.group_id, 1_000, 3, Some(0)).unwrap();
        assert_eq!(bounded.expires_ms, 1_000 + MIN_INVITE_TTL_MS);
    }

    #[test]
    fn finalize_rejects_unknown_invite() {
        let (db, founder) = setup();
        let bob = Identity::generate_with_pow_bits(1);
        let created = create_group(&db, &founder, "Guilde", 0).unwrap();
        assert!(finalize_invite_accept(
            &db,
            &founder,
            &created.group_id,
            &[0xEE; 16],
            &[0; 32],
            &bob.public_key(),
            1,
        )
        .is_err());
    }

    /// Régression CRITICAL/HIGH (liens publics) : seul le créateur d'une
    /// invitation la finalise. Un membre INVITE distinct qui reçoit un redeem
    /// valide (l'attaquant peut le diffuser à plusieurs membres) ne produit ni
    /// `AddMember` ni clé scellée — la course inter-nœuds de fuite de clé est
    /// éliminée. Le créateur, lui, finalise normalement.
    #[test]
    fn only_creator_finalizes_redeem_not_other_invite_members() {
        let (db, founder) = setup();
        let alice = Identity::generate_with_pow_bits(1);
        let bob = Identity::generate_with_pow_bits(1);
        let created = create_group(&db, &founder, "Guilde", 0).unwrap();

        // Le fondateur autorise une invitation (créateur = fondateur) et admet
        // Alice comme membre.
        let inv_alice = author_invite_create(&db, &founder, &created.group_id, 0, 0).unwrap();
        finalize_invite_accept(
            &db,
            &founder,
            &created.group_id,
            &inv_alice.invite_id,
            &inv_alice.secret,
            &alice.public_key(),
            1,
        )
        .unwrap()
        .expect("le créateur admet Alice");

        // Alice reçoit un rôle portant la permission INVITE : elle est donc un
        // membre INVITE légitime, mais PAS la créatrice du lien ci-dessous.
        let role_id = [0x33u8; 16];
        author_op(
            &db,
            &founder,
            &created.group_id,
            &GroupOpBody::AddRole {
                role_id,
                name: "inviteurs".into(),
                color: 0,
                position: 1,
                permissions: perms::INVITE,
            },
            2,
        )
        .unwrap();
        author_op(
            &db,
            &founder,
            &created.group_id,
            &GroupOpBody::AssignRole {
                member: alice.public_key(),
                role_id,
            },
            3,
        )
        .unwrap();
        assert!(group_state(&db, &created.group_id)
            .unwrap()
            .can(&alice.public_key(), perms::INVITE));

        // Invitation « lien public » créée par le fondateur (usages illimités).
        let link = author_invite_create_with(&db, &founder, &created.group_id, 4, 0, None).unwrap();

        // Alice (membre INVITE, mais non créatrice) reçoit un redeem valide :
        // cas neutre `Ok(None)`, aucun membre ajouté, aucune clé scellée.
        let via_alice = finalize_invite_accept(
            &db,
            &alice,
            &created.group_id,
            &link.invite_id,
            &link.secret,
            &bob.public_key(),
            5,
        )
        .unwrap();
        assert!(
            via_alice.is_none(),
            "un membre INVITE non créateur ne finalise jamais"
        );
        assert!(!group_state(&db, &created.group_id)
            .unwrap()
            .is_member(&bob.public_key()));

        // Le créateur (fondateur) finalise normalement : Bob est admis, clé
        // scellée.
        let via_founder = finalize_invite_accept(
            &db,
            &founder,
            &created.group_id,
            &link.invite_id,
            &link.secret,
            &bob.public_key(),
            6,
        )
        .unwrap();
        assert!(via_founder.is_some(), "le créateur finalise le rachat");
        assert!(group_state(&db, &created.group_id)
            .unwrap()
            .is_member(&bob.public_key()));
    }

    /// Régression LOW : un texte encodé démesuré est rejeté proprement (erreur,
    /// jamais de panique) AVANT tout décodage base64url.
    #[test]
    fn oversized_invite_link_is_rejected_before_decode() {
        let huge = format!(
            "{INVITE_LINK_PREFIX}{}",
            "A".repeat(MAX_INVITE_LINK_ENCODED_LEN + 1)
        );
        assert!(decode_invite_link(&huge).is_err());
        // Sans préfixe non plus.
        let huge_bare = "A".repeat(MAX_INVITE_LINK_ENCODED_LEN + 100);
        assert!(decode_invite_link(&huge_bare).is_err());
    }
}
