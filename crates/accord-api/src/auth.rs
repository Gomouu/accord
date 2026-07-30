//! Jeton d'authentification de l'API locale.
//!
//! Le serveur n'écoute que sur `127.0.0.1`, mais tout processus local peut
//! s'y connecter : un jeton de session aléatoire (32 octets, hexadécimal)
//! est exigé en première requête (`auth`). La comparaison est en temps
//! constant. Le jeton n'est jamais journalisé.

use rand::RngCore;
use subtle::ConstantTimeEq;

/// Jeton de session de l'API locale.
#[derive(Clone)]
pub struct AuthToken(String);

impl AuthToken {
    /// Génère un jeton aléatoire (CSPRNG, 256 bits, hexadécimal).
    pub fn generate() -> Self {
        use std::fmt::Write;
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let mut hex = String::with_capacity(64);
        for b in bytes {
            // `write!` dans la chaîne finale plutôt qu'un `format!` par octet :
            // une seule allocation au lieu de trente-trois.
            let _ = write!(hex, "{b:02x}");
        }
        Self(hex)
    }

    /// Reconstruit un jeton connu (lecture du fichier de session du démon).
    pub fn from_string(token: String) -> Self {
        Self(token)
    }

    /// Valeur à transmettre au client légitime (UI).
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Comparaison en temps constant avec un jeton présenté.
    pub fn verify(&self, presented: &str) -> bool {
        self.0.as_bytes().ct_eq(presented.as_bytes()).into()
    }
}

impl std::fmt::Debug for AuthToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AuthToken(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_unique_and_verify() {
        let a = AuthToken::generate();
        let b = AuthToken::generate();
        assert_ne!(a.expose(), b.expose());
        assert_eq!(a.expose().len(), 64);
        assert!(a.verify(a.expose()));
        assert!(!a.verify(b.expose()));
        assert!(!a.verify(""));
    }

    #[test]
    fn debug_never_prints_token() {
        let a = AuthToken::generate();
        assert_eq!(format!("{a:?}"), "AuthToken(***)");
    }
}
