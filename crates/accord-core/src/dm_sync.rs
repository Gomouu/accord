//! Anti-entropie des conversations directes **entre les appareils d'un même
//! compte** (`docs/MULTI_DEVICE.md` §7, étape 2).
//!
//! Un appareil qui se rallume demande à ses autres machines ce qu'il a manqué.
//! La forme est celle de l'anti-entropie des op-logs de groupe
//! ([`crate::group::sync_offer`]) : une offre annonce ce qu'on détient, la
//! machine en retard demande la suite, l'autre la sert.
//!
//! Trois choix décident de tout le reste.
//!
//! 🔒 **L'empreinte est bornée à une conversation.** Deux appareils d'un compte
//! n'ont pas les mêmes messages sortants tant qu'ils n'ont pas convergé : une
//! empreinte de toute la base ne tomberait donc jamais juste, et déclencherait
//! un rattrapage complet à chaque passe, indéfiniment.
//!
//! 🔒 **La fenêtre est bornée à [`SYNC_WINDOW`] messages par conversation.**
//! C'est un *rattrapage*, pas un transfert d'historique — celui-ci est un
//! chantier à part (§7, étape 3 : long, explicite, avec une barre de
//! progression). Le répondeur ne sert jamais au-delà de sa propre fenêtre, ce
//! qui plafonne d'office ce qu'une conversation peut transférer.
//!
//! ⚠️ **Les horloges de Lamport de deux appareils ne sont pas comparables.**
//! Un message reçu porte celle du pair (identique partout), mais un message
//! sortant porte celle de la machine qui l'a composé, et chaque machine a la
//! sienne. Une position n'a donc de sens que **dans une conversation** : c'est
//! pourquoi rien ici n'accepte une position globale.

use sha2::{Digest, Sha256};

use crate::db::{Db, DmRecord};
use crate::error::CoreError;

/// Messages les plus récents d'une conversation couverts par le rattrapage.
///
/// Aligné sur [`accord_proto::core_msg::MAX_SELF_SYNC_ITEMS`] : le répondeur ne
/// sert que sa fenêtre et le fil ne transporte pas plus d'éléments par demande,
/// donc une passe suffit à donner tout ce qu'un appareil peut donner. Les
/// désaligner rendrait la convergence dépendante du nombre de passes, c'est-à-
/// dire du hasard des reconnexions.
pub const SYNC_WINDOW: usize = accord_proto::core_msg::MAX_SELF_SYNC_ITEMS as usize;

/// Ce qu'un appareil détient d'une conversation, résumé pour comparaison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncOffer {
    /// Conversation visée (clé de compte du pair).
    pub conv: [u8; 32],
    /// Messages couverts par l'empreinte.
    pub count: u32,
    /// Position la plus élevée de la fenêtre.
    pub max_lamport: u64,
    /// SHA-256 des `msg_id` de la fenêtre, en ordre canonique croissant.
    pub digest: [u8; 32],
}

/// Fenêtre canonique d'une conversation : les [`SYNC_WINDOW`] messages les plus
/// récents, rendus du plus ancien au plus récent.
///
/// L'ordre `(lamport, msg_id)` est le même que celui de l'historique — deux
/// appareils qui détiennent le même ensemble en tirent donc la même suite, ce
/// qui est toute la raison d'être d'une empreinte.
fn window(db: &Db, conv: &[u8; 32]) -> Result<Vec<DmRecord>, CoreError> {
    let mut msgs = db.dm_history(conv, u64::MAX, SYNC_WINDOW)?;
    msgs.reverse();
    Ok(msgs)
}

/// Offre d'anti-entropie pour une conversation.
pub fn sync_offer(db: &Db, conv: &[u8; 32]) -> Result<SyncOffer, CoreError> {
    let msgs = window(db, conv)?;
    let mut hasher = Sha256::new();
    for m in &msgs {
        hasher.update(m.msg_id);
    }
    Ok(SyncOffer {
        conv: *conv,
        count: msgs.len() as u32,
        max_lamport: msgs.iter().map(|m| m.lamport).max().unwrap_or(0),
        digest: hasher.finalize().into(),
    })
}

/// Vrai si l'offre reçue décrit autre chose que ce qu'on détient.
///
/// Le nombre est comparé en plus de l'empreinte, comme pour les op-logs : deux
/// ensembles distincts de même empreinte supposeraient une collision SHA-256,
/// mais la comparaison ne coûte rien et rend le refus explicite.
///
/// Une divergence ne dit pas *qui* est en retard : chacun offre de son côté et
/// chacun tire ce qui lui manque, donc les deux sens se règlent tout seuls.
pub fn diverges(local: &SyncOffer, remote: &SyncOffer) -> bool {
    local.digest != remote.digest || local.count != remote.count
}

/// Messages à servir en réponse à une demande `since_lamport` / `max_items`.
///
/// Pris **dans la fenêtre**, de position strictement supérieure à
/// `since_lamport`, du plus ancien au plus récent. L'ordre croissant n'est pas
/// cosmétique : il permet au demandeur d'avancer son curseur sur le dernier
/// élément reçu et de reprendre exactement là, sans trou, si sa demande a été
/// tronquée.
pub fn items_for_pull(
    db: &Db,
    conv: &[u8; 32],
    since_lamport: u64,
    max_items: usize,
) -> Result<Vec<DmRecord>, CoreError> {
    Ok(window(db, conv)?
        .into_iter()
        .filter(|m| m.lamport > since_lamport)
        .take(max_items)
        .collect())
}

/// Messages d'une conversation **strictement sous** une borne, du plus ancien
/// au plus récent, au plus `max`.
///
/// 🔴 **Ce n'est pas [`items_for_pull`], et la différence est tout le sujet du
/// transfert d'historique.** `items_for_pull` sert *dans* [`window`] — les
/// [`SYNC_WINDOW`] messages les plus RÉCENTS — et filtre par borne BASSE.
/// Avancer son curseur ne fait donc que rétrécir : il n'existe aucune suite
/// d'appels qui atteigne un message plus ancien que la fenêtre. C'est correct
/// pour ce qu'il fait — un rattrapage entre deux appareils déjà garnis — et
/// inutilisable pour garnir un appareil vide.
///
/// Ici la borne est HAUTE et la page descend : appelé à répétition avec la
/// position la plus basse déjà reçue, on parcourt l'historique entier, du
/// récent vers l'ancien, sans jamais charger plus de `max` lignes.
///
/// ⚠️ Aucune empreinte n'accompagne ce parcours, à dessein. Une empreinte a du
/// sens sur une fenêtre que les deux côtés peuvent calculer ; sur un historique
/// entier elle coûterait une lecture complète à chaque page, et le demandeur —
/// qui part de rien — n'a de toute façon rien à comparer.
pub fn items_before(
    db: &Db,
    conv: &[u8; 32],
    before_lamport: u64,
    max: usize,
) -> Result<Vec<DmRecord>, CoreError> {
    // `dm_history` est déjà « les N plus récents strictement sous la borne ».
    let mut msgs = db.dm_history(conv, before_lamport, max)?;
    // Rendus en ordre croissant, comme `items_for_pull` : une seule convention
    // d'ordre sur le fil évite d'avoir à se demander laquelle s'applique.
    msgs.reverse();
    Ok(msgs)
}

/// Insère un message rattrapé depuis un autre appareil du compte.
///
/// Rend vrai si la ligne était nouvelle. 🔒 L'insertion est un `INSERT OR
/// IGNORE` sur `msg_id`, et c'est **elle seule** qui garantit l'absence de
/// doublon après un rattrapage : le `msg_id` transporté est celui d'origine,
/// jamais un identifiant neuf. Passer par la composition d'un message direct en
/// frapperait un nouveau à chaque passe et dupliquerait la conversation en
/// silence.
///
/// L'horloge locale est avancée sur la position reçue, comme à l'ingestion d'un
/// message du réseau : sans cela, cette machine frapperait ensuite des positions
/// déjà utilisées ailleurs.
pub fn ingest_item(db: &Db, search_key: &[u8; 32], record: &DmRecord) -> Result<bool, CoreError> {
    db.bump_lamport(record.lamport)?;
    if !db.insert_dm(record)? {
        return Ok(false);
    }
    // Provenance locale : cette machine n'a pas composé ce message, donc elle
    // n'a rien à dire de sa livraison (voir `Db::mark_dm_synced`).
    db.mark_dm_synced(&record.msg_id)?;
    // Pièces jointes et index de recherche, comme à l'ingestion ordinaire —
    // sans quoi le message rattrapé serait affiché sans ses fichiers et
    // resterait introuvable à la recherche. Un corps qui ne se décode pas en
    // texte (carte d'invitation locale, genre plus récent) est inséré tel quel
    // et n'a simplement rien à indexer.
    if let Ok(accord_proto::core_msg::MsgBody::Text {
        text, attachments, ..
    }) = accord_proto::core_msg::MsgBody::decode_body(record.kind, &record.body)
    {
        db.put_msg_attachments(&record.msg_id, &attachments)?;
        crate::search::index_message(db, search_key, &record.msg_id, &text)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use accord_proto::core_msg::MsgBody;

    fn db() -> Db {
        Db::open_in_memory(&[7u8; 32]).expect("base en mémoire")
    }

    fn texte(n: u8) -> Vec<u8> {
        MsgBody::Text {
            text: format!("message {n}"),
            reply_to: None,
            attachments: vec![],
        }
        .encode_body()
    }

    /// Une conversation de `n` messages, positions 1..=n.
    fn conversation_de(db: &Db, conv: [u8; 32], n: u64) -> Result<(), CoreError> {
        for i in 1..=n {
            let r = record(conv, conv, i as u8, i);
            ingest_item(db, &[3u8; 32], &r)?;
        }
        Ok(())
    }

    #[test]
    fn le_rattrapage_ne_peut_pas_descendre_sous_sa_fenetre() {
        // 🔴 Le test qui justifie tout le transfert d'historique. La feuille de
        // route affirmait qu'il suffisait de « piloter en boucle » le
        // rattrapage. Voici ce que cette boucle donne réellement.
        let db = db();
        let conv = [0xC0; 32];
        let total = (SYNC_WINDOW as u64) + 40;
        conversation_de(&db, conv, total).expect("conversation");

        // Passe 1 : le rattrapage rend le HAUT de l'historique, jamais plus.
        let p1 = items_for_pull(&db, &conv, 0, SYNC_WINDOW).expect("passe 1");
        assert_eq!(p1.len(), SYNC_WINDOW);
        let plus_bas_servi = p1.iter().map(|m| m.lamport).min().expect("non vide");
        assert_eq!(
            plus_bas_servi,
            total - SYNC_WINDOW as u64 + 1,
            "la fenêtre est ancrée sur le haut"
        );

        // Passe 2, curseur avancé comme le ferait la boucle décrite : vide.
        let plus_haut = p1.iter().map(|m| m.lamport).max().expect("non vide");
        let p2 = items_for_pull(&db, &conv, plus_haut, SYNC_WINDOW).expect("passe 2");
        assert!(p2.is_empty(), "avancer le curseur ne peut que rétrécir");

        // Et il n'existe AUCUNE valeur de `since_lamport` qui atteigne le bas :
        // baisser le curseur ne fait que réservir la même fenêtre.
        let p3 = items_for_pull(&db, &conv, 0, SYNC_WINDOW).expect("passe 3");
        assert_eq!(
            p3.iter().map(|m| m.lamport).min(),
            Some(plus_bas_servi),
            "les 40 messages du bas sont inatteignables par ce chemin"
        );
    }

    #[test]
    fn la_page_descendante_parcourt_tout_lhistorique() {
        let db = db();
        let conv = [0xC1; 32];
        let total = (SYNC_WINDOW as u64) + 40;
        conversation_de(&db, conv, total).expect("conversation");

        // La boucle du transfert : partir du haut, redescendre page par page.
        let mut vus: Vec<u64> = Vec::new();
        let mut borne = u64::MAX;
        loop {
            let page = items_before(&db, &conv, borne, SYNC_WINDOW).expect("page");
            if page.is_empty() {
                break;
            }
            // Ordre croissant sur le fil, comme `items_for_pull` : une seule
            // convention, sinon le destinataire doit deviner laquelle.
            let positions: Vec<u64> = page.iter().map(|m| m.lamport).collect();
            let mut triees = positions.clone();
            triees.sort_unstable();
            assert_eq!(positions, triees, "page rendue en ordre croissant");

            borne = page.iter().map(|m| m.lamport).min().expect("non vide");
            vus.extend(positions);
        }
        vus.sort_unstable();

        assert_eq!(vus, (1..=total).collect::<Vec<_>>(), "tout, sans trou");
        // Et la borne est bien STRICTE : sans quoi la boucle ne finirait pas.
        assert!(items_before(&db, &conv, 1, SYNC_WINDOW)
            .expect("sous le premier")
            .is_empty());
    }

    fn record(conv: [u8; 32], author: [u8; 32], id: u8, lamport: u64) -> DmRecord {
        DmRecord {
            msg_id: [id; 16],
            peer: conv,
            author,
            lamport,
            sent_ms: 1_700_000_000_000 + lamport,
            kind: 0,
            body: texte(id),
            acked: true,
            deleted: false,
            edited: None,
        }
    }

    #[test]
    fn deux_bases_de_meme_contenu_ont_la_meme_empreinte() {
        let conv = [0xC0; 32];
        let moi = [1u8; 32];
        let (a, b) = (db(), db());
        // Insérées dans des ordres opposés : l'ordre canonique doit gommer la
        // différence, sinon l'empreinte dépendrait de l'ordre d'arrivée et ne
        // tomberait juste que par chance.
        for id in [1u8, 2, 3] {
            a.insert_dm(&record(conv, moi, id, u64::from(id))).unwrap();
        }
        for id in [3u8, 1, 2] {
            b.insert_dm(&record(conv, moi, id, u64::from(id))).unwrap();
        }
        let (oa, ob) = (
            sync_offer(&a, &conv).unwrap(),
            sync_offer(&b, &conv).unwrap(),
        );
        assert_eq!(oa, ob);
        assert!(!diverges(&oa, &ob));
        assert_eq!(oa.count, 3);
        assert_eq!(oa.max_lamport, 3);
    }

    #[test]
    fn lempreinte_est_bornee_a_la_conversation() {
        // 🔒 Le message d'une AUTRE conversation ne doit pas entrer dans
        // l'empreinte : deux appareils dont seule une autre conversation
        // diffère se croiraient sinon en désaccord pour toujours.
        let (conv, autre) = ([0xC0; 32], [0xC1; 32]);
        let moi = [1u8; 32];
        let (a, b) = (db(), db());
        for base in [&a, &b] {
            base.insert_dm(&record(conv, moi, 1, 1)).unwrap();
        }
        a.insert_dm(&record(autre, moi, 9, 9)).unwrap();
        assert!(!diverges(
            &sync_offer(&a, &conv).unwrap(),
            &sync_offer(&b, &conv).unwrap()
        ));
    }

    #[test]
    fn un_message_de_plus_fait_diverger_et_se_sert_dans_lordre() {
        let conv = [0xC0; 32];
        let moi = [1u8; 32];
        let (a, b) = (db(), db());
        for id in 1u8..=3 {
            a.insert_dm(&record(conv, moi, id, u64::from(id))).unwrap();
        }
        b.insert_dm(&record(conv, moi, 1, 1)).unwrap();

        assert!(diverges(
            &sync_offer(&b, &conv).unwrap(),
            &sync_offer(&a, &conv).unwrap()
        ));
        let servis = items_for_pull(&a, &conv, 1, SYNC_WINDOW).unwrap();
        assert_eq!(
            servis.iter().map(|m| m.lamport).collect::<Vec<_>>(),
            vec![2, 3],
            "croissant et strictement au-delà du curseur"
        );

        // Bornage : une demande d'un seul élément rend le plus ancien manquant,
        // celui par lequel il faut reprendre.
        let un = items_for_pull(&a, &conv, 1, 1).unwrap();
        assert_eq!(un.len(), 1);
        assert_eq!(un[0].lamport, 2);
    }

    #[test]
    fn la_fenetre_plafonne_ce_quun_appareil_peut_servir() {
        let conv = [0xC0; 32];
        let moi = [1u8; 32];
        let base = db();
        for id in 0..(SYNC_WINDOW as u64 + 10) {
            let mut r = record(conv, moi, 0, id + 1);
            r.msg_id = [0u8; 16];
            r.msg_id[..8].copy_from_slice(&(id + 1).to_be_bytes());
            base.insert_dm(&r).unwrap();
        }
        let offre = sync_offer(&base, &conv).unwrap();
        assert_eq!(offre.count as usize, SYNC_WINDOW);
        // Depuis zéro, le répondeur ne donne QUE sa fenêtre : le rattrapage
        // n'est pas un transfert d'historique.
        let servis = items_for_pull(&base, &conv, 0, SYNC_WINDOW).unwrap();
        assert_eq!(servis.len(), SYNC_WINDOW);
        assert_eq!(servis[0].lamport, 11);
    }

    #[test]
    fn ingerer_deux_fois_le_meme_message_ninsere_quune_ligne() {
        let conv = [0xC0; 32];
        let base = db();
        let r = record(conv, conv, 4, 4);
        assert!(ingest_item(&base, &[3u8; 32], &r).unwrap());
        assert!(!ingest_item(&base, &[3u8; 32], &r).unwrap());
        assert_eq!(base.dm_history(&conv, u64::MAX, 10).unwrap().len(), 1);
        assert!(base.dm_synced_set(&conv).unwrap().contains(&r.msg_id));
    }
}
