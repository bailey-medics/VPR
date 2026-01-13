//! Internal implementation of UUID services.
//!
//! This module contains the implementation details for UUID and timestamp-based
//! unique identifiers used throughout the VPR system.

use crate::{UuidError, UuidResult};
use chrono::{DateTime, Duration, Utc};
use std::path::{Path, PathBuf};
use std::{fmt, str::FromStr};

/// Re-exported for convenience.
pub use ::uuid::Uuid;

/// VPR's canonical UUID representation (32 lowercase hex characters, no hyphens).
///
/// This wrapper type guarantees that once constructed, the contained UUID is in VPR's
/// canonical format. It provides type safety for UUID operations and ensures consistent
/// path derivation across the system.
///
/// # When to use this type
/// Use this wrapper whenever you are:
/// - Accepting a UUID string from *outside* the core (CLI input, API request, etc), or
/// - Deriving a sharded storage path for a patient.
/// - Generating new patient identifiers.
///
/// Once you have a `UuidService`, you can safely assume the internal UUID is valid
/// and in canonical form.
///
/// # Construction
/// - [`UuidService::new`] generates a new canonical UUID (for new patient records).
/// - [`UuidService::parse`] validates an externally supplied identifier.
///
/// # Errors
/// [`UuidService::parse`] returns [`UuidError::InvalidInput`] if the input is not already
/// canonical.
///
/// # Display format
/// When displayed or converted to string, `UuidService` always produces the canonical
/// 32-character lowercase hex format without hyphens.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UuidService(Uuid);

impl Default for UuidService {
    fn default() -> Self {
        Self::new()
    }
}

impl UuidService {
    /// Generates a new UUID in VPR's canonical form.
    ///
    /// This is suitable for allocating a fresh identifier during patient creation.
    /// The generated UUID is cryptographically secure and follows RFC 4122 version 4.
    ///
    /// # Returns
    ///
    /// Returns a newly generated canonical UUID wrapped in `UuidService`.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Validates and parses a UUID string that must already be in VPR's canonical form.
    ///
    /// This does **not** normalise other common UUID forms (for example, hyphenated or uppercase).
    /// Callers must provide the canonical representation. This strict validation ensures
    /// consistency and prevents issues with different UUID representations.
    ///
    /// # Arguments
    ///
    /// * `input` - UUID string to validate and wrap. Must be exactly 32 lowercase hex characters.
    ///
    /// # Returns
    ///
    /// Returns a validated [`UuidService`] on success.
    ///
    /// # Errors
    ///
    /// Returns [`UuidError::InvalidInput`] if `input` is not in canonical form.
    pub fn parse(input: &str) -> UuidResult<Self> {
        if Self::is_canonical(input) {
            // SAFETY: is_canonical guarantees valid hex, so parse_str will succeed
            let uuid = Uuid::parse_str(input).expect("is_canonical guarantees valid UUID");
            return Ok(Self(uuid));
        }
        Err(UuidError::InvalidInput(format!(
            "UUID must be 32 lowercase hex characters without hyphens, got: '{}'",
            input
        )))
    }

    /// Returns the UUID as a `uuid::Uuid`.
    ///
    /// This method provides access to the underlying `uuid::Uuid` for operations
    /// that require the standard UUID library interface.
    ///
    /// # Returns
    ///
    /// Returns a copy of the inner UUID.
    ///
    /// # Note
    ///
    /// The returned UUID is guaranteed to be valid since `UuidService` only
    /// contains validated UUIDs.
    pub fn uuid(&self) -> Uuid {
        self.0
    }

    /// Returns true if `input` is in VPR's canonical UUID form.
    ///
    /// This is a purely syntactic check that validates:
    /// - Exactly 32 bytes long
    /// - Contains only lowercase hex characters (`0-9` and `a-f`)
    ///
    /// This method is fast and can be used for pre-validation before calling [`parse`].
    ///
    /// # Arguments
    ///
    /// * `input` - Candidate UUID string to validate.
    ///
    /// # Returns
    ///
    /// Returns `true` if `input` is canonical, otherwise `false`.
    pub fn is_canonical(input: &str) -> bool {
        input.len() == 32
            && input
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    }

    /// Returns `parent_dir/<s1>/<s2>/<uuid>/` where `s1`/`s2` are derived from this UUID.
    ///
    /// This implements VPR's sharding scheme:
    /// - `s1` is the first two hex characters of the UUID
    /// - `s2` is the next two hex characters
    /// - The full UUID forms the leaf directory
    ///
    /// This sharding prevents filesystem performance issues with large numbers of patient
    /// directories in a single location.
    ///
    /// # Arguments
    ///
    /// * `parent_dir` - Base directory under which to shard the UUID.
    ///
    /// # Returns
    ///
    /// Returns the fully qualified sharded directory path for this UUID.
    pub fn sharded_dir(&self, parent_dir: &Path) -> PathBuf {
        let canonical = self.0.simple().to_string();
        let s1 = &canonical[0..2];
        let s2 = &canonical[2..4];
        parent_dir.join(s1).join(s2).join(&canonical)
    }
}

impl fmt::Display for UuidService {
    /// Formats the UUID in canonical form (32 lowercase hex characters, no hyphens).
    ///
    /// This ensures consistent string representation across the application.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display in canonical (simple) form
        write!(f, "{}", self.0.simple())
    }
}

impl FromStr for UuidService {
    type Err = UuidError;

    /// Parses a string into a `UuidService`, requiring canonical form.
    ///
    /// This is equivalent to calling [`UuidService::parse`].
    ///
    /// # Errors
    ///
    /// Returns [`UuidError::InvalidInput`] if the string is not in canonical UUID form.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        UuidService::parse(s)
    }
}

/// A time-prefixed timestamp identifier.
///
/// Format:
/// `YYYYMMDDTHHMMSS.mmmZ-<canonical_uuid>`
///
/// Example:
/// `20260111T143522.045Z-550e8400e29b41d4a716446655440000`
///
/// This identifier is:
/// - Globally unique (UUID)
/// - Human-readable
/// - Monotonic per patient when generated inside a per-patient lock
///
/// # Monotonicity Guarantee
///
/// When calling [`TimestampUuid::generate`] with the previous timestamp UID,
/// the timestamp is guaranteed to be strictly greater than the previous one
/// (incremented by at least 1ms if necessary). This ensures correct ordering
/// of compositions within a patient record.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub struct TimestampUuid {
    timestamp: DateTime<Utc>,
    uuid: UuidService,
}

impl TimestampUuid {
    /// Returns the timestamp component of this timestamp UID.
    #[allow(dead_code)]
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Returns a reference to the UUID component of this timestamp UID.
    #[allow(dead_code)]
    pub fn uuid(&self) -> &UuidService {
        &self.uuid
    }
}

impl FromStr for TimestampUuid {
    type Err = UuidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (ts_str, uuid_str) = s.split_once('-').ok_or_else(|| {
            UuidError::InvalidInput(format!("Invalid timestamp UID format: '{}'", s))
        })?;

        // Parse the timestamp portion (without the Z suffix)
        if !ts_str.ends_with('Z') {
            return Err(UuidError::InvalidInput(format!(
                "Timestamp must end with 'Z': '{}'",
                ts_str
            )));
        }

        let ts_no_z = &ts_str[..ts_str.len() - 1];
        let naive =
            chrono::NaiveDateTime::parse_from_str(ts_no_z, "%Y%m%dT%H%M%S%.3f").map_err(|e| {
                UuidError::InvalidInput(format!("Invalid timestamp format '{}': {}", ts_str, e))
            })?;

        let timestamp = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);

        let uuid = UuidService::parse(uuid_str)?;

        Ok(Self { timestamp, uuid })
    }
}

impl fmt::Display for TimestampUuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}",
            self.timestamp.format("%Y%m%dT%H%M%S%.3fZ"),
            self.uuid
        )
    }
}

impl TimestampUuid {
    /// Generate a new timestamp UID.
    ///
    /// If `last_uid` is provided, the timestamp is guaranteed to be
    /// strictly greater than the last one (by at least 1 ms).
    ///
    /// This is designed to be called **inside a per-patient lock**.
    #[allow(dead_code)]
    pub fn generate(last_uid: Option<&TimestampUuid>) -> Self {
        let now = Utc::now();

        let timestamp = match last_uid {
            Some(prev) if now <= prev.timestamp => prev.timestamp + Duration::milliseconds(1),
            _ => now,
        };

        Self {
            timestamp,
            uuid: UuidService::new(),
        }
    }
}

impl TimestampUuid {
    #[allow(dead_code)]
    pub fn generate_from_str(last_uid: Option<&str>) -> UuidResult<Self> {
        let parsed = match last_uid {
            Some(s) => Some(TimestampUuid::from_str(s)?),
            None => None,
        };

        Ok(Self::generate(parsed.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_generates_valid_uuid() {
        let uuid_service = UuidService::new();
        let canonical = uuid_service.to_string();

        // Verify the generated UUID is in canonical form
        assert_eq!(canonical.len(), 32);
        assert!(UuidService::is_canonical(&canonical));
    }

    #[test]
    fn test_parse_valid_canonical_uuid() {
        let canonical = "550e8400e29b41d4a716446655440000";
        let result = UuidService::parse(canonical);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), canonical);
    }

    #[test]
    fn test_parse_rejects_hyphenated_uuid() {
        let hyphenated = "550e8400-e29b-41d4-a716-446655440000";
        let result = UuidService::parse(hyphenated);

        assert!(result.is_err());
        match result {
            Err(UuidError::InvalidInput(msg)) => {
                assert!(msg.contains("32 lowercase hex characters"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_parse_rejects_uppercase_uuid() {
        let uppercase = "550E8400E29B41D4A716446655440000";
        let result = UuidService::parse(uppercase);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rejects_mixed_case_uuid() {
        let mixed = "550e8400E29b41d4A716446655440000";
        let result = UuidService::parse(mixed);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rejects_too_short() {
        let short = "550e8400e29b41d4a71644665544000";
        let result = UuidService::parse(short);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rejects_too_long() {
        let long = "550e8400e29b41d4a7164466554400000";
        let result = UuidService::parse(long);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rejects_invalid_characters() {
        let invalid = "550e8400e29b41d4a716446655440zzz";
        let result = UuidService::parse(invalid);

        assert!(result.is_err());
    }

    #[test]
    fn test_is_canonical_valid() {
        assert!(UuidService::is_canonical(
            "550e8400e29b41d4a716446655440000"
        ));
        assert!(UuidService::is_canonical(
            "00000000000000000000000000000000"
        ));
        assert!(UuidService::is_canonical(
            "ffffffffffffffffffffffffffffffff"
        ));
    }

    #[test]
    fn test_is_canonical_invalid() {
        // Uppercase
        assert!(!UuidService::is_canonical(
            "550E8400E29B41D4A716446655440000"
        ));

        // Hyphenated
        assert!(!UuidService::is_canonical(
            "550e8400-e29b-41d4-a716-446655440000"
        ));

        // Too short
        assert!(!UuidService::is_canonical(
            "550e8400e29b41d4a71644665544000"
        ));

        // Too long
        assert!(!UuidService::is_canonical(
            "550e8400e29b41d4a7164466554400000"
        ));

        // Invalid characters
        assert!(!UuidService::is_canonical(
            "550e8400e29b41d4a716446655440zzz"
        ));

        // Empty string
        assert!(!UuidService::is_canonical(""));
    }

    #[test]
    fn test_sharded_dir_structure() {
        let uuid = UuidService::parse("550e8400e29b41d4a716446655440000").unwrap();
        let parent = Path::new("/patient_data/clinical");
        let sharded = uuid.sharded_dir(parent);

        assert_eq!(
            sharded,
            PathBuf::from("/patient_data/clinical/55/0e/550e8400e29b41d4a716446655440000")
        );
    }

    #[test]
    fn test_sharded_dir_different_uuids() {
        let uuid1 = UuidService::parse("00112233445566778899aabbccddeeff").unwrap();
        let uuid2 = UuidService::parse("aabbccddeeff00112233445566778899").unwrap();

        let parent = Path::new("/data");

        let sharded1 = uuid1.sharded_dir(parent);
        let sharded2 = uuid2.sharded_dir(parent);

        assert_eq!(
            sharded1,
            PathBuf::from("/data/00/11/00112233445566778899aabbccddeeff")
        );
        assert_eq!(
            sharded2,
            PathBuf::from("/data/aa/bb/aabbccddeeff00112233445566778899")
        );
        assert_ne!(sharded1, sharded2);
    }

    #[test]
    fn test_display_format() {
        let uuid = UuidService::parse("550e8400e29b41d4a716446655440000").unwrap();
        let displayed = format!("{}", uuid);

        assert_eq!(displayed, "550e8400e29b41d4a716446655440000");
        assert!(UuidService::is_canonical(&displayed));
    }

    #[test]
    fn test_from_str_valid() {
        let canonical = "550e8400e29b41d4a716446655440000";
        let result: Result<UuidService, _> = canonical.parse();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), canonical);
    }

    #[test]
    fn test_from_str_invalid() {
        let hyphenated = "550e8400-e29b-41d4-a716-446655440000";
        let result: Result<UuidService, _> = hyphenated.parse();

        assert!(result.is_err());
    }

    #[test]
    fn test_uuid_method_returns_valid_uuid() {
        let uuid_service = UuidService::parse("550e8400e29b41d4a716446655440000").unwrap();
        let inner_uuid = uuid_service.uuid();

        // Verify the inner UUID matches the canonical form
        assert_eq!(
            inner_uuid.simple().to_string(),
            "550e8400e29b41d4a716446655440000"
        );
    }

    #[test]
    fn test_round_trip_new_to_string_to_parse() {
        let original = UuidService::new();
        let as_string = original.to_string();
        let parsed = UuidService::parse(&as_string).unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_clone_and_equality() {
        let uuid1 = UuidService::parse("550e8400e29b41d4a716446655440000").unwrap();
        let uuid2 = uuid1.clone();

        assert_eq!(uuid1, uuid2);
    }

    #[test]
    fn test_hash_consistency() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let uuid1 = UuidService::parse("550e8400e29b41d4a716446655440000").unwrap();
        let uuid2 = UuidService::parse("550e8400e29b41d4a716446655440000").unwrap();

        let mut hasher1 = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();

        uuid1.hash(&mut hasher1);
        uuid2.hash(&mut hasher2);

        assert_eq!(hasher1.finish(), hasher2.finish());
    }

    #[test]
    fn test_debug_format() {
        let uuid = UuidService::parse("550e8400e29b41d4a716446655440000").unwrap();
        let debug = format!("{:?}", uuid);

        // Debug format should contain the UUID value
        assert!(debug.contains("550e8400"));
    }

    // TimestampUuid tests

    #[test]
    fn test_timestamp_uid_generate_new() {
        let uid = TimestampUuid::generate(None);

        // Should have a valid UUID component
        let uuid_str = uid.uuid().to_string();
        assert_eq!(uuid_str.len(), 32);
        assert!(UuidService::is_canonical(&uuid_str));
    }

    #[test]
    fn test_timestamp_uid_generate_monotonic() {
        let uid1 = TimestampUuid::generate(None);
        std::thread::sleep(std::time::Duration::from_millis(10));
        let uid2 = TimestampUuid::generate(Some(&uid1));

        // Second timestamp should be strictly greater
        assert!(uid2.timestamp() > uid1.timestamp());
    }

    #[test]
    fn test_timestamp_uid_generate_monotonic_same_instant() {
        let uid1 = TimestampUuid::generate(None);
        // Don't sleep - force the monotonic increment logic
        let uid2 = TimestampUuid::generate(Some(&uid1));

        // Even with no elapsed time, second should be strictly later
        assert!(uid2.timestamp() > uid1.timestamp());
    }

    #[test]
    fn test_timestamp_uid_display_format() {
        let uid = TimestampUuid::generate(None);
        let displayed = uid.to_string();

        // Should contain a hyphen separator
        assert!(displayed.contains('-'));

        // Should end with 'Z' in the timestamp portion
        assert!(displayed.starts_with("20"));

        // Should be parseable
        let parts: Vec<&str> = displayed.split('-').collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[0].ends_with('Z'));
        assert!(UuidService::is_canonical(parts[1]));
    }

    #[test]
    fn test_timestamp_uid_parse_valid() {
        let valid = "20260111T143522.045Z-550e8400e29b41d4a716446655440000";
        let result = TimestampUuid::from_str(valid);

        assert!(result.is_ok());
        let uid = result.unwrap();
        assert_eq!(uid.uuid().to_string(), "550e8400e29b41d4a716446655440000");
    }

    #[test]
    fn test_timestamp_uid_parse_missing_hyphen() {
        let invalid = "20260111T143522.045Z550e8400e29b41d4a716446655440000";
        let result = TimestampUuid::from_str(invalid);

        assert!(result.is_err());
        match result {
            Err(UuidError::InvalidInput(msg)) => {
                assert!(msg.contains("Invalid timestamp UID format"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_timestamp_uid_parse_missing_z_suffix() {
        let invalid = "20260111T143522.045-550e8400e29b41d4a716446655440000";
        let result = TimestampUuid::from_str(invalid);

        assert!(result.is_err());
        match result {
            Err(UuidError::InvalidInput(msg)) => {
                assert!(msg.contains("must end with 'Z'"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_timestamp_uid_parse_invalid_timestamp() {
        let invalid = "20260199T143522.045Z-550e8400e29b41d4a716446655440000";
        let result = TimestampUuid::from_str(invalid);

        assert!(result.is_err());
        match result {
            Err(UuidError::InvalidInput(msg)) => {
                assert!(msg.contains("Invalid timestamp format"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_timestamp_uid_parse_invalid_uuid() {
        let invalid = "20260111T143522.045Z-not-a-valid-uuid";
        let result = TimestampUuid::from_str(invalid);

        assert!(result.is_err());
    }

    #[test]
    fn test_timestamp_uid_round_trip() {
        // Use a specific timestamp that has no sub-millisecond precision
        // to ensure clean round-trip through the %.3f format
        let original_str = "20260111T143522.045Z-550e8400e29b41d4a716446655440000";
        let original = TimestampUuid::from_str(original_str).unwrap();
        let as_string = original.to_string();
        let parsed = TimestampUuid::from_str(&as_string).unwrap();

        assert_eq!(as_string, original_str);
        assert_eq!(original, parsed);
        assert_eq!(original.timestamp(), parsed.timestamp());
        assert_eq!(original.uuid(), parsed.uuid());
    }

    #[test]
    fn test_timestamp_uid_generate_from_str_with_previous() {
        let prev = "20260111T143522.045Z-550e8400e29b41d4a716446655440000";
        let result = TimestampUuid::generate_from_str(Some(prev));

        assert!(result.is_ok());
        let new_uid = result.unwrap();
        let prev_uid = TimestampUuid::from_str(prev).unwrap();

        assert!(new_uid.timestamp() > prev_uid.timestamp());
    }

    #[test]
    fn test_timestamp_uid_generate_from_str_without_previous() {
        let result = TimestampUuid::generate_from_str(None);

        assert!(result.is_ok());
        let uid = result.unwrap();
        assert!(UuidService::is_canonical(&uid.uuid().to_string()));
    }

    #[test]
    fn test_timestamp_uid_generate_from_str_invalid() {
        let invalid = "not-a-valid-timestamp-uid";
        let result = TimestampUuid::generate_from_str(Some(invalid));

        assert!(result.is_err());
    }

    #[test]
    fn test_timestamp_uid_equality() {
        let uid1 = TimestampUuid::from_str("20260111T143522.045Z-550e8400e29b41d4a716446655440000")
            .unwrap();
        let uid2 = TimestampUuid::from_str("20260111T143522.045Z-550e8400e29b41d4a716446655440000")
            .unwrap();

        assert_eq!(uid1, uid2);
    }

    #[test]
    fn test_timestamp_uid_clone() {
        let uid1 = TimestampUuid::generate(None);
        let uid2 = uid1.clone();

        assert_eq!(uid1, uid2);
    }
}
