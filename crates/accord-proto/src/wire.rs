//! Primitives d'encodage/décodage binaire strictes (SPEC §0).
//!
//! Tout entier multi-octets est big-endian. Le décodage est strict : toute
//! longueur hors bornes, tout UTF-8 invalide ou tout octet excédentaire rejette
//! la structure entière.

use crate::limits;
use thiserror::Error;

/// Erreur de décodage. Côté réseau, un paquet indécodable est rejeté en silence.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum DecodeError {
    /// Le tampon se termine avant la fin de la structure attendue.
    #[error("fin de tampon inattendue")]
    UnexpectedEof,
    /// Des octets restent après la fin de la structure.
    #[error("octets excédentaires en fin de structure")]
    TrailingBytes,
    /// Une chaîne n'est pas de l'UTF-8 valide.
    #[error("UTF-8 invalide")]
    InvalidUtf8,
    /// Une longueur déclarée dépasse la borne du champ.
    #[error("longueur hors bornes: {0}")]
    TooLarge(&'static str),
    /// Un discriminant ou une valeur de champ est hors du domaine autorisé.
    #[error("valeur invalide: {0}")]
    InvalidValue(&'static str),
    /// La version de protocole du paquet dépasse celle supportée.
    #[error("version de protocole non supportée: {0}")]
    UnsupportedVersion(u8),
}

/// Accumulateur d'octets pour l'encodage.
#[derive(Default, Debug)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    /// Crée un writer vide.
    pub fn new() -> Self {
        Self::default()
    }

    /// Crée un writer avec une capacité initiale.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap),
        }
    }

    /// Consomme le writer et rend le tampon encodé.
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    /// Longueur courante du tampon.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Vrai si aucun octet n'a été écrit.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Écrit un `u8`.
    pub fn put_u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    /// Écrit un `u16` big-endian.
    pub fn put_u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    /// Écrit un `u32` big-endian.
    pub fn put_u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    /// Écrit un `u64` big-endian.
    pub fn put_u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    /// Écrit un tableau de taille fixe (`bytes<N>`).
    pub fn put_arr<const N: usize>(&mut self, v: &[u8; N]) {
        self.buf.extend_from_slice(v);
    }

    /// Écrit des octets bruts sans préfixe de longueur.
    pub fn put_raw(&mut self, v: &[u8]) {
        self.buf.extend_from_slice(v);
    }

    /// Écrit un `vbytes` (longueur u16 puis octets). Panique si > 65 535 —
    /// les appelants valident les bornes métier avant l'encodage.
    pub fn put_vbytes(&mut self, v: &[u8]) {
        debug_assert!(v.len() <= u16::MAX as usize, "vbytes trop long");
        self.put_u16(v.len() as u16);
        self.put_raw(v);
    }

    /// Écrit un `lbytes` (longueur u32 puis octets).
    pub fn put_lbytes(&mut self, v: &[u8]) {
        debug_assert!(v.len() <= limits::MAX_LBYTES, "lbytes trop long");
        self.put_u32(v.len() as u32);
        self.put_raw(v);
    }

    /// Écrit une `str` (vbytes UTF-8).
    pub fn put_str(&mut self, v: &str) {
        self.put_vbytes(v.as_bytes());
    }

    /// Écrit un `opt<T>` via une closure d'encodage.
    pub fn put_opt<T>(&mut self, v: Option<&T>, f: impl FnOnce(&mut Self, &T)) {
        match v {
            None => self.put_u8(0),
            Some(inner) => {
                self.put_u8(1);
                f(self, inner);
            }
        }
    }

    /// Écrit une `list<T>` via une closure d'encodage par élément.
    pub fn put_list<T>(&mut self, items: &[T], mut f: impl FnMut(&mut Self, &T)) {
        debug_assert!(items.len() <= limits::MAX_LIST, "liste trop longue");
        self.put_u16(items.len() as u16);
        for item in items {
            f(self, item);
        }
    }
}

/// Curseur de lecture strict sur un tampon.
#[derive(Debug)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Crée un reader sur le tampon complet.
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Octets restants à lire.
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// Erreur si des octets restent (fin de structure stricte).
    pub fn finish(&self) -> Result<(), DecodeError> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(DecodeError::TrailingBytes)
        }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if self.remaining() < n {
            return Err(DecodeError::UnexpectedEof);
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    /// Lit un `u8`.
    pub fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }

    /// Lit un `u16` big-endian.
    pub fn u16(&mut self) -> Result<u16, DecodeError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    /// Lit un `u32` big-endian.
    pub fn u32(&mut self) -> Result<u32, DecodeError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Lit un `u64` big-endian.
    pub fn u64(&mut self) -> Result<u64, DecodeError> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_be_bytes(a))
    }

    /// Lit un tableau de taille fixe (`bytes<N>`).
    pub fn arr<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let b = self.take(N)?;
        let mut a = [0u8; N];
        a.copy_from_slice(b);
        Ok(a)
    }

    /// Lit le reste du tampon (utilisé pour les ciphertexts en fin de paquet).
    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.buf[self.pos..];
        self.pos = self.buf.len();
        s
    }

    /// Lit un `vbytes` borné à `max` octets.
    pub fn vbytes(&mut self, max: usize, what: &'static str) -> Result<Vec<u8>, DecodeError> {
        let len = self.u16()? as usize;
        if len > max {
            return Err(DecodeError::TooLarge(what));
        }
        Ok(self.take(len)?.to_vec())
    }

    /// Lit un `lbytes` borné à `max` octets.
    pub fn lbytes(&mut self, max: usize, what: &'static str) -> Result<Vec<u8>, DecodeError> {
        let len = self.u32()? as usize;
        if len > max.min(limits::MAX_LBYTES) {
            return Err(DecodeError::TooLarge(what));
        }
        Ok(self.take(len)?.to_vec())
    }

    /// Lit une `str` UTF-8 bornée à `max` octets.
    pub fn str(&mut self, max: usize, what: &'static str) -> Result<String, DecodeError> {
        let raw = self.vbytes(max, what)?;
        String::from_utf8(raw).map_err(|_| DecodeError::InvalidUtf8)
    }

    /// Lit un `opt<T>` via une closure de décodage.
    pub fn opt<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, DecodeError>,
    ) -> Result<Option<T>, DecodeError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(f(self)?)),
            _ => Err(DecodeError::InvalidValue("opt tag")),
        }
    }

    /// Lit un `opt<T>` **additif de fin de structure** : rend `None` sans
    /// rien consommer si le tampon est déjà épuisé, au lieu d'échouer. Un
    /// émetteur plus ancien qui ignore un champ ajouté après coup n'écrit
    /// aucun octet pour lui ; ce champ manquant doit donc décoder à `None`
    /// plutôt que rejeter tout le message (rétrocompatibilité filaire). Si au
    /// moins un octet reste, décode normalement le tag `opt`. Réservé aux
    /// champs strictement en fin de variant — un champ suivi d'autres champs
    /// doit toujours utiliser [`Reader::opt`].
    pub fn opt_tail<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, DecodeError>,
    ) -> Result<Option<T>, DecodeError> {
        if self.remaining() == 0 {
            return Ok(None);
        }
        self.opt(f)
    }

    /// Lit un id court optionnel de fin de structure (`opt<str>`) en MEILLEUR
    /// EFFORT : le champ additif se décode comme les autres ([`Reader::opt_tail`]
    /// — absent chez un émetteur plus ancien → `None`), mais un id dont la
    /// longueur déclarée dépasse `max` octets, dont le contenu n'est pas de
    /// l'UTF-8, ou que `valid` rejette, est réduit à `None` AU LIEU de faire
    /// échouer tout le message. Réservé aux champs annexes strictement en fin
    /// de variant (décoration d'avatar, effet de profil) : ces ids traversent
    /// la frontière de confiance P2P, et un pair malveillant ne doit jamais
    /// pouvoir invalider le profil entier avec un id trop long ou hors
    /// alphabet. Aucune allocation pour un id rejeté (le contenu n'est
    /// qu'emprunté puis abandonné) ; le flux reste toujours aligné (les octets
    /// déclarés sont consommés). Un tag `opt` invalide (ni 0 ni 1) ou une
    /// vraie troncature (moins d'octets que la longueur déclarée) restent des
    /// erreurs.
    pub fn opt_tail_short_id(
        &mut self,
        max: usize,
        valid: impl FnOnce(&str) -> bool,
    ) -> Result<Option<String>, DecodeError> {
        if self.remaining() == 0 {
            return Ok(None);
        }
        match self.u8()? {
            0 => Ok(None),
            1 => {
                let len = self.u16()? as usize;
                let bytes = self.take(len)?;
                match std::str::from_utf8(bytes) {
                    Ok(s) if s.len() <= max && valid(s) => Ok(Some(s.to_string())),
                    _ => Ok(None),
                }
            }
            _ => Err(DecodeError::InvalidValue("opt tag")),
        }
    }

    /// Lit une `list<T>` bornée à `max` éléments.
    pub fn list<T>(
        &mut self,
        max: usize,
        what: &'static str,
        mut f: impl FnMut(&mut Self) -> Result<T, DecodeError>,
    ) -> Result<Vec<T>, DecodeError> {
        let n = self.u16()? as usize;
        if n > max.min(limits::MAX_LIST) {
            return Err(DecodeError::TooLarge(what));
        }
        let mut out = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            out.push(f(self)?);
        }
        Ok(out)
    }
}

/// Type encodable vers le format filaire.
pub trait WireEncode {
    /// Écrit la représentation filaire de `self`.
    fn encode(&self, w: &mut Writer);

    /// Encode vers un tampon autonome.
    fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        self.encode(&mut w);
        w.into_bytes()
    }
}

/// Type décodable depuis le format filaire.
pub trait WireDecode: Sized {
    /// Lit une valeur depuis le reader (sans exiger la fin du tampon).
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError>;

    /// Décode un tampon entier ; les octets excédentaires sont une erreur.
    fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(bytes);
        let v = Self::decode(&mut r)?;
        r.finish()?;
        Ok(v)
    }
}
