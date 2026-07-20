//! Cible fuzz : décodage des messages du canal FILE et des manifests signés.
//!
//! Invariant : `FileMsg::from_bytes` et `Manifest::from_bytes` ne paniquent
//! jamais sur une entrée arbitraire — la liste de hachages de blocs et les
//! blobs sont bornés avant allocation (SPEC §0, §13). Surface exposée à toute
//! source de fichier (essaim, pièces jointes, médias de profil).

#![no_main]

use accord_proto::file_msg::{FileMsg, Manifest};
use accord_proto::WireDecode;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = FileMsg::from_bytes(data);
    let _ = Manifest::from_bytes(data);
});
