//! Pont macOS natif : autorisation micro (TCC) via AVFoundation.
//!
//! Seul crate de l'espace de travail autorisé à contenir de l'`unsafe` : il
//! se limite à deux appels Objective-C documentés d'`AVCaptureDevice`
//! (`authorizationStatusForMediaType:` et `requestAccessForMediaType:`),
//! sans jamais ouvrir de flux de capture. L'invite système n'existe qu'à
//! l'état « indéterminé » ; lire l'état réel ici permet à l'UI de ne jamais
//! re-déclencher l'invite à mauvais escient (redemandes en boucle sur un
//! bundle mal signé, voir docs/DISTRIBUTION.md § signature locale stable).
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
        /// Constante `AVMediaTypeAudio` d'AVFoundation.
        ///
        /// Lire une `static` externe est `unsafe` : sa validité repose sur le
        /// fait que le TYPE déclaré ici corresponde à celui du symbole, et que
        /// le symbole soit initialisé avant tout accès. Les deux tiennent : le
        /// symbole est un `NSString *` constant du framework (donc une
        /// référence non nulle, immuable, de durée de vie statique), et le
        /// framework est lié à ce binaire — l'éditeur de liens dynamique
        /// l'initialise avant `main`. Aucun accès n'a lieu avant, ce module
        /// n'ayant ni initialiseur statique ni constructeur.
        static AVMediaTypeAudio: &'static NSString;
    }

    pub fn etat() -> &'static str {
        let Some(cls) = AnyClass::get(c"AVCaptureDevice") else {
            return super::ETAT_INCONNU;
        };
        // SAFETY : `cls` est la classe AVCaptureDevice, résolue à l'instant et
        // donc vivante (les classes Objective-C d'un framework chargé ne sont
        // jamais libérées). `authorizationStatusForMediaType:` est une méthode
        // de CLASSE documentée, qui prend un `AVMediaType` (soit un
        // `NSString *` — la constante du framework, voir sa déclaration
        // ci-dessus) et rend un `NSInteger` : `isize` en est la représentation
        // exacte sur toutes les cibles Apple. La méthode est sûre à appeler
        // depuis n'importe quel fil et ne déclenche AUCUNE invite système.
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
        // SAFETY : mêmes invariants que `etat()` pour `cls` et pour la
        // constante `AVMediaTypeAudio`. `requestAccessForMediaType:
        // completionHandler:` est une méthode de CLASSE documentée qui rend
        // `void` — d'où le `let () =`, qui interdit toute mélecture du retour.
        // Le second argument est un pointeur de bloc valide : `&*bloc` emprunte
        // le `RcBlock` qui vit jusqu'à la fin de cette fonction, et
        // AVFoundation en prend une COPIE (contrat des blocs de complétion
        // Objective-C) avant de rendre la main — la copie survit donc à
        // l'emprunt. Le bloc est appelé exactement une fois, sur une file
        // arbitraire : la fermeture est `Send`, ne capture qu'un `Mutex`, et le
        // `take()` la rend idempotente si le contrat était violé.
        let () = unsafe {
            msg_send![cls, requestAccessForMediaType: AVMediaTypeAudio, completionHandler: &*bloc]
        };
        // Attente de la réponse : bloque le fil appelant, jamais le fil
        // principal (contrat documenté sur `micro_demander_bloquant`).
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
