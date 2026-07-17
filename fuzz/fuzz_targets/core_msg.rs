//! Cible fuzz : décodage des messages du canal CORE.
//!
//! Invariant : `CoreMsg::from_bytes` (décodage strict : octets excédentaires
//! rejetés) ne panique jamais sur une entrée arbitraire.

#![no_main]

use accord_proto::core_msg::{CoreMsg, MsgBody};
use accord_proto::WireDecode;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = CoreMsg::from_bytes(data);
    // Corps applicatif d'un message : même schéma kind + bytes que GroupOpBody.
    if let Some((&kind, body)) = data.split_first() {
        let _ = MsgBody::decode_body(kind, body);
    }
});
