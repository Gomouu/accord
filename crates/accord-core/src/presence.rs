//! Rich presence (SPEC §6, `PRESENCE` 0x08): own status persisted in the
//! meta table, wire mapping and custom-status validation.
//!
//! The wire only carries the four SPEC statuses (0=online, 1=idle, 2=dnd,
//! 3=offline). "Invisible" is a purely local choice: the node keeps working
//! normally but always announces `offline` to its friends.

use crate::db::Db;
use crate::error::CoreError;

/// Meta key of the persisted own status byte.
const META_STATUS_KEY: &str = "presence.status";
/// Meta key of the persisted custom status text (absent or empty = none).
const META_CUSTOM_KEY: &str = "presence.custom";

/// Upper bound of the custom status text, in UTF-8 bytes — aligned with the
/// wire decode bound of `presence.custom` (SPEC §6, 0x08).
pub const MAX_CUSTOM_STATUS_BYTES: usize = 256;

/// Own presence status chosen by the local user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnStatus {
    /// Online (green).
    Online,
    /// Idle / away (yellow).
    Idle,
    /// Do not disturb (red).
    Dnd,
    /// Invisible: announced as offline, node fully functional.
    Invisible,
}

impl OwnStatus {
    /// Parses the API identifier (`friends.set_status`).
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "online" => Ok(Self::Online),
            "idle" => Ok(Self::Idle),
            "dnd" => Ok(Self::Dnd),
            "invisible" => Ok(Self::Invisible),
            _ => Err(CoreError::Invalid("statut de présence inconnu")),
        }
    }

    /// API identifier (stable, exposed by `friends.get_status`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Idle => "idle",
            Self::Dnd => "dnd",
            Self::Invisible => "invisible",
        }
    }

    /// Status byte announced on the wire (SPEC §6, 0x08): invisible is
    /// indistinguishable from offline for the peers.
    pub fn wire_status(&self) -> u8 {
        match self {
            Self::Online => 0,
            Self::Idle => 1,
            Self::Dnd => 2,
            Self::Invisible => 3,
        }
    }

    fn to_byte(self) -> u8 {
        // Local storage encoding (NOT the wire encoding: 3 means invisible
        // here, while the wire uses 3 for offline).
        match self {
            Self::Online => 0,
            Self::Idle => 1,
            Self::Dnd => 2,
            Self::Invisible => 3,
        }
    }

    fn from_byte(byte: u8) -> Self {
        match byte {
            1 => Self::Idle,
            2 => Self::Dnd,
            3 => Self::Invisible,
            _ => Self::Online,
        }
    }
}

/// Maps a peer wire status (0-3) to its API identifier (`friends.list`,
/// `event.presence`). Unknown values degrade to `offline`.
pub fn status_str(status: u8) -> &'static str {
    match status {
        0 => "online",
        1 => "idle",
        2 => "dnd",
        _ => "offline",
    }
}

/// Validates and canonicalizes a custom status text: trimmed, no control
/// characters, bounded to [`MAX_CUSTOM_STATUS_BYTES`] UTF-8 bytes (wire
/// decode bound). An empty (or whitespace-only) text clears the status.
pub fn validate_custom_status(text: &str) -> Result<String, CoreError> {
    let trimmed = text.trim();
    if trimmed.len() > MAX_CUSTOM_STATUS_BYTES {
        return Err(CoreError::Invalid("statut personnalisé trop long"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(CoreError::Invalid(
            "statut personnalisé avec caractères de contrôle",
        ));
    }
    Ok(trimmed.to_string())
}

/// Persists the own presence status. `custom`: `None` keeps the current
/// text, `Some("")` (after trim) clears it, `Some(text)` replaces it.
/// All-or-nothing: the custom text is validated before any write.
pub fn set_own_presence(db: &Db, status: OwnStatus, custom: Option<&str>) -> Result<(), CoreError> {
    let canonical = custom.map(validate_custom_status).transpose()?;
    db.set_meta(META_STATUS_KEY, &[status.to_byte()])?;
    if let Some(text) = canonical {
        db.set_meta(META_CUSTOM_KEY, text.as_bytes())?;
    }
    Ok(())
}

/// Reads the persisted own presence. Defaults to `(Online, None)` when
/// nothing was ever set.
pub fn own_presence(db: &Db) -> Result<(OwnStatus, Option<String>), CoreError> {
    let status = db
        .meta(META_STATUS_KEY)?
        .and_then(|v| v.first().copied())
        .map(OwnStatus::from_byte)
        .unwrap_or(OwnStatus::Online);
    let custom = match db.meta(META_CUSTOM_KEY)? {
        Some(bytes) if !bytes.is_empty() => String::from_utf8(bytes).ok(),
        _ => None,
    };
    Ok((status, custom))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        Db::open_in_memory(&[9u8; 32]).unwrap()
    }

    #[test]
    fn own_presence_defaults_to_online_without_custom_text() {
        let db = db();
        assert_eq!(own_presence(&db).unwrap(), (OwnStatus::Online, None));
    }

    #[test]
    fn set_own_presence_roundtrips_and_keeps_or_clears_custom_text() {
        let db = db();
        set_own_presence(&db, OwnStatus::Dnd, Some("  focus mode  ")).unwrap();
        assert_eq!(
            own_presence(&db).unwrap(),
            (OwnStatus::Dnd, Some("focus mode".into()))
        );
        // `None` keeps the current text.
        set_own_presence(&db, OwnStatus::Idle, None).unwrap();
        assert_eq!(
            own_presence(&db).unwrap(),
            (OwnStatus::Idle, Some("focus mode".into()))
        );
        // Empty text clears it.
        set_own_presence(&db, OwnStatus::Invisible, Some("")).unwrap();
        assert_eq!(own_presence(&db).unwrap(), (OwnStatus::Invisible, None));
    }

    #[test]
    fn custom_status_is_validated_before_any_write() {
        let db = db();
        set_own_presence(&db, OwnStatus::Online, Some("ok")).unwrap();
        let too_long = "x".repeat(MAX_CUSTOM_STATUS_BYTES + 1);
        assert!(set_own_presence(&db, OwnStatus::Dnd, Some(&too_long)).is_err());
        assert!(set_own_presence(&db, OwnStatus::Dnd, Some("a\u{0007}b")).is_err());
        // Failed update: previous status and text are untouched.
        assert_eq!(
            own_presence(&db).unwrap(),
            (OwnStatus::Online, Some("ok".into()))
        );
    }

    #[test]
    fn wire_status_hides_invisible_as_offline() {
        assert_eq!(OwnStatus::Online.wire_status(), 0);
        assert_eq!(OwnStatus::Idle.wire_status(), 1);
        assert_eq!(OwnStatus::Dnd.wire_status(), 2);
        assert_eq!(OwnStatus::Invisible.wire_status(), 3);
    }

    #[test]
    fn parse_and_as_str_are_inverse() {
        for s in [
            OwnStatus::Online,
            OwnStatus::Idle,
            OwnStatus::Dnd,
            OwnStatus::Invisible,
        ] {
            assert_eq!(OwnStatus::parse(s.as_str()).unwrap(), s);
        }
        assert!(OwnStatus::parse("offline").is_err());
    }

    #[test]
    fn peer_status_string_degrades_unknown_to_offline() {
        assert_eq!(status_str(0), "online");
        assert_eq!(status_str(1), "idle");
        assert_eq!(status_str(2), "dnd");
        assert_eq!(status_str(3), "offline");
        assert_eq!(status_str(200), "offline");
    }
}
