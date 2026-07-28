//! Mesures statiques du jalon 6 : mémoire à cinq serveurs et coût d'adhésion
//! en fonction de la longueur du journal. Sorties du banc principal, qui
//! franchissait les 800 lignes — le cliquet refuse toute nouvelle entrée.

use super::*;

/// Cinq serveurs de 200 membres tenus EN MÊME TEMPS.
///
/// Critère de fin du jalon 6 : « la mémoire reste sous le budget avec 5
/// serveurs de 200 membres » (budget §10.2 : 400 Mo). Il était non mesuré, et
/// l'instrument existait déjà — d'où cette fonction plutôt qu'un banc neuf.
///
/// ⚠️ **Ce que ce chiffre est, et n'est pas.** Il compte les octets alloués et
/// non rendus pendant le repli des cinq états, compteur allumé : la structure
/// dominante, pas la RSS du processus. Une vraie RSS inclurait le runtime
/// Tokio, les tampons du transport, la webview — rien de tout ça n'est ici.
/// Lire ce nombre comme « la mémoire de l'application » serait faux ; il borne
/// par le bas, et c'est déjà ce qu'on cherchait à savoir.
pub(super) fn mesure_cinq_serveurs() {
    const SERVEURS: usize = 5;
    const MEMBRES: usize = 200;

    let serveurs: Vec<Serveur> = (0..SERVEURS).map(|_| peupler(MEMBRES)).collect();
    // Les cinq états vivent ensemble jusqu'à la fin de la mesure : les replier
    // l'un après l'autre en laissant tomber le précédent mesurerait le plus
    // gros des cinq, pas leur somme.
    let (etats, octets) = memoire_de(|| {
        serveurs
            .iter()
            .map(|s| GroupState::fold(&s.ops))
            .collect::<Vec<_>>()
    });
    assert_eq!(etats.len(), SERVEURS, "cinq états tenus ensemble");
    for etat in &etats {
        assert_eq!(etat.members.len(), MEMBRES + 1, "membres attendus");
    }

    let mo = octets as f64 / (1024.0 * 1024.0);
    println!(
        "  {SERVEURS} serveurs x {MEMBRES} membres, etats replies simultanes : \
{octets} octets ({mo:.1} Mo) — budget 400 Mo (ROADMAP 10.2)"
    );
    // Aucune assertion sur le budget : un banc qui échoue sur un seuil devient
    // un test, et un test de perf sur une machine partagée est instable. Le
    // chiffre est imprimé, sa lecture appartient à `docs/PERFORMANCE.md`.
    drop(etats);
}

/// Coût d'une adhésion en fonction de la LONGUEUR du journal, à nombre de
/// membres constant.
///
/// C'est la question que la compaction pose vraiment (ROADMAP §18.3 A) : son
/// scénario n'est pas « beaucoup de membres » mais « un serveur avec des
/// années d'historique de configuration ». Les paliers de `PALIERS` font varier
/// les membres ; celui-ci fait varier les ops, ce que rien ne mesurait.
///
/// Le remplissage supplémentaire est une suite de `SetMeta` — le changement de
/// configuration le plus banal d'un serveur qui vit. Chaque op est signée et
/// vérifiée comme n'importe quelle autre : c'est bien le coût réel d'un
/// journal long, pas un remplissage artificiellement bon marché.
pub(super) fn mesure_journal_long() {
    for ops_sup in [0usize, 2_000, 10_000] {
        let mut serveur = peupler(200);
        let identity = Identity::from_seed_with_pow_bits(GRAINE_FONDATEUR, POW_BANC);
        // Connexion propre sur la même base : `Serveur` n'expose pas la sienne.
        let db = Db::open(&serveur.chemin, &CLE_BASE).expect("base");
        let mut horloge = INSTANT_BANC + 10_000_000;
        for i in 0..ops_sup {
            horloge += 1_000;
            let op = group::author_op(
                &db,
                &identity,
                &serveur.group_id,
                &GroupOpBody::SetMeta {
                    name: format!("Serveur du banc {i}"),
                    icon: None,
                    banner_color: Some(0x0058_65F2),
                },
                horloge,
            )
            .expect("op refusée");
            serveur.ops.push(op);
        }

        // Redémarrage : la base contient déjà le journal, les signatures ont
        // été vérifiées à l'ingestion. Ce chiffre isole donc le repli pur —
        // c'est LUI qu'un instantané persisté localement supprimerait, sans
        // toucher au protocole ni demander de faire confiance à quiconque.
        {
            let db = Db::open(&serveur.chemin, &CLE_BASE).expect("base");
            let debut = std::time::Instant::now();
            let etat = group::group_state(&db, &serveur.group_id).expect("etat");
            println!(
                "  journal de {} ops : repli a froid (redemarrage) {:?} — {} membres",
                serveur.ops.len(),
                debut.elapsed(),
                etat.members.len()
            );
        }

        let joignant = joignant_vierge(&serveur.group_id);
        let debut = std::time::Instant::now();
        let membres = rejoindre(
            &joignant,
            &serveur.fondateur,
            &serveur.group_id,
            &serveur.ops,
        );
        let duree = debut.elapsed();
        println!(
            "  journal de {} ops (200 membres) : adhesion {:?} — {} membres",
            serveur.ops.len(),
            duree,
            membres
        );
    }
}
