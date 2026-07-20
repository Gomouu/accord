//! Property test (proptest) : repli DÉTERMINISTE du CRDT de groupe (SPEC §6.2).
//!
//! Invariant central du log d'op répliqué : deux pairs qui détiennent le MÊME
//! ensemble d'ops convergent vers le MÊME état, quel que soit l'ORDRE dans
//! lequel ils les ont reçues. [`GroupState::fold`] trie sur l'ordre total
//! `(lamport, node_id(author), op_id)` avant d'appliquer : le repli doit donc
//! être invariant par permutation de l'entrée (les ops portent des `op_id`
//! distincts, comme le garantit la déduplication par clé primaire à
//! l'ingestion).

use accord_core::group::state::GroupState;
use accord_proto::core_msg::GroupOp;
use proptest::prelude::*;

/// Stratégie d'un op de groupe au corps opaque : `op_id` imposé par l'appelant
/// (distinct par op), le reste arbitraire. Les signatures ne sont pas vérifiées
/// par `fold` (elles le sont à l'ingestion) : un corps aléatoire exerce autant
/// le tri et l'application (les ops non applicables sont ignorées sans panique).
fn op_strategy(op_id: [u8; 16]) -> impl Strategy<Value = GroupOp> {
    (
        any::<[u8; 16]>(),
        any::<u64>(),
        any::<u64>(),
        any::<[u8; 32]>(),
        any::<u8>(),
        prop::collection::vec(any::<u8>(), 0..64),
    )
        .prop_map(
            move |(group_id, lamport, wall_ms, author, kind, body)| GroupOp {
                op_id,
                group_id,
                lamport,
                wall_ms,
                author,
                kind,
                body,
                sig: [0; 64],
            },
        )
}

/// Un log d'ops à `op_id` DISTINCTS (index sur les 2 premiers octets) plus une
/// permutation de ce log, pour comparer les deux replis.
fn log_et_permutation() -> impl Strategy<Value = (Vec<GroupOp>, Vec<usize>)> {
    (1usize..24)
        .prop_flat_map(|n| {
            let ops: Vec<_> = (0..n)
                .map(|i| {
                    let mut id = [0u8; 16];
                    id[0] = (i & 0xFF) as u8;
                    id[1] = (i >> 8) as u8;
                    op_strategy(id)
                })
                .collect();
            (ops, Just(n))
        })
        .prop_flat_map(|(ops, n)| (Just(ops), Just((0..n).collect::<Vec<_>>()).prop_shuffle()))
}

proptest! {
    /// `fold` est invariant par permutation : même ensemble d'ops ⇒ même état,
    /// indépendamment de l'ordre de réception.
    #[test]
    fn fold_invariant_par_permutation((ops, perm) in log_et_permutation()) {
        let permutees: Vec<GroupOp> = perm.iter().map(|&i| ops[i].clone()).collect();
        let a = GroupState::fold(&ops);
        let b = GroupState::fold(&permutees);
        prop_assert_eq!(a, b);
    }

    /// `fold` est idempotent sur l'ordre déjà canonique (repli stable).
    #[test]
    fn fold_stable_sur_replis_repetes((ops, _perm) in log_et_permutation()) {
        let a = GroupState::fold(&ops);
        let b = GroupState::fold(&ops);
        prop_assert_eq!(a, b);
    }

    /// Appliquer un op arbitraire sur un état neuf ne panique jamais (le repli
    /// reste total quel que soit le corps décodé).
    #[test]
    fn apply_ne_panique_pas(op in op_strategy([9; 16])) {
        let mut state = GroupState::default();
        let _ = state.apply(&op);
    }
}
