//! Pont macOS natif : autorisation micro (TCC) via AVFoundation.
//!
//! Seul crate de l'espace de travail autorisé à contenir de l'`unsafe` : il
//! se limite à deux appels Objective-C documentés d'`AVCaptureDevice`
//! (`authorizationStatusForMediaType:` et `requestAccessForMediaType:`),
//! sans jamais ouvrir de flux de capture. L'invite système n'existe qu'à
//! l'état « indéterminé » ; lire l'état réel ici permet à l'UI de ne jamais
//! re-déclencher l'invite à mauvais escient (redemandes en boucle sur un
//! bundle mal signé, voir DISTRIBUTION.md § signature locale stable).
//!
//! Hors macOS, les deux fonctions rendent des valeurs neutres
//! (`"unsupported"` / erreur explicite) — aucun lien AVFoundation n'est émis.

/// État de l'autorisation micro, aligné sur `AVAuthorizationStatus`.
pub const ETAT_INDETERMINE: &str = "undetermined";
/// Accès restreint par une politique système (contrôle parental, MDM).
pub const ETAT_RESTREINT: &str = "restricted";
/// Accès refusé par l'utilisateur.
pub const ETAT_REFUSE: &str = "denied";
/// Accès accordé.
pub const ETAT_ACCORDE: &str = "granted";
/// Plateforme sans TCC ou AVFoundation indisponible.
pub const ETAT_INCONNU: &str = "unsupported";

#[cfg(target_os = "macos")]
mod ffi {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, Bool};
    use objc2_foundation::NSString;
    use std::sync::mpsc;
    use std::sync::Mutex;

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {
        static AVMediaTypeAudio: &'static NSString;
    }

    pub fn etat() -> &'static str {
        let Some(cls) = AnyClass::get(c"AVCaptureDevice") else {
            return super::ETAT_INCONNU;
        };
        // SAFETY : méthode de classe documentée d'AVCaptureDevice ; le type
        // média est la constante AVFoundation elle-même, le retour est un
        // NSInteger (AVAuthorizationStatus ∈ 0..=3).
        let statut: isize =
            unsafe { msg_send![cls, authorizationStatusForMediaType: AVMediaTypeAudio] };
        match statut {
            0 => super::ETAT_INDETERMINE,
            1 => super::ETAT_RESTREINT,
            2 => super::ETAT_REFUSE,
            3 => super::ETAT_ACCORDE,
            _ => super::ETAT_INCONNU,
        }
    }

    pub fn demander_bloquant() -> Result<bool, String> {
        let Some(cls) = AnyClass::get(c"AVCaptureDevice") else {
            return Err("AVFoundation indisponible".into());
        };
        let (tx, rx) = mpsc::channel::<bool>();
        let tx = Mutex::new(Some(tx));
        let bloc = block2::RcBlock::new(move |accorde: Bool| {
            if let Some(tx) = tx.lock().ok().and_then(|mut g| g.take()) {
                let _ = tx.send(accorde.as_bool());
            }
        });
        // SAFETY : méthode de classe documentée ; le bloc de complétion est
        // retenu (`RcBlock`) et AVFoundation l'appelle exactement une fois,
        // sur une file arbitraire — le canal mpsc est fait pour ça.
        let () = unsafe {
            msg_send![cls, requestAccessForMediaType: AVMediaTypeAudio, completionHandler: &*bloc]
        };
        rx.recv().map_err(|_| "demande interrompue".into())
    }
}

/// État courant de l'autorisation micro (`granted`, `denied`, `undetermined`,
/// `restricted`, `unsupported`). Jamais d'invite déclenchée.
pub fn micro_etat() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        ffi::etat()
    }
    #[cfg(not(target_os = "macos"))]
    {
        ETAT_INCONNU
    }
}

/// Déclenche l'invite micro système et BLOQUE jusqu'à la réponse de
/// l'utilisateur (à appeler hors du fil principal). Sans invite possible
/// (état déjà tranché), AVFoundation répond immédiatement avec l'état acquis.
pub fn micro_demander_bloquant() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        ffi::demander_bloquant()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("demande d'autorisation micro non prise en charge ici".into())
    }
}
