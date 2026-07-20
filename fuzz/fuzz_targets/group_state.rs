//! Cible fuzz : décodage d'une op de groupe puis repli du CRDT.
//!
//! Invariant : décoder une `GroupOp` arbitraire puis la replier
//! (`GroupState::fold`) et l'appliquer (`GroupState::apply`) ne panique jamais.
//! Le repli du log d'op est le point où un membre malveillant pourrait tenter
//! de forger un état incohérent (permissions, salons, fils) — il doit rester
//! total et sans allocation non bornée (SPEC §5, listes plafonnées à 4096).

#![no_main]

use accord_core::group::state::GroupState;
use accord_proto::core_msg::GroupOp;
use accord_proto::WireDecode;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(op) = GroupOp::from_bytes(data) else {
        return;
    };
    // Repli à partir de l'op seule (chemin `fold`), puis application
    // incrémentale sur un état neuf (chemin `apply`) : les deux doivent être
    // totaux quelle que soit l'op décodée.
    let _ = GroupState::fold(std::slice::from_ref(&op));
    let mut state = GroupState::default();
    let _ = state.apply(&op);
});
