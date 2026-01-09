//! UUID and sharded-path utilities.
//!
//! VPR stores patient records under sharded directories derived from a UUID.
//!
//! To keep path derivation deterministic and consistent across the codebase, VPR uses a *canonical*
//! UUID representation for storage identifiers: **32 lowercase hexadecimal characters** (no
//! hyphens).
//!
//! This module provides:
//! - A small wrapper type ([`UuidService`]) that *guarantees* the canonical format once
//!   constructed.
//! - Shared sharding logic to derive patient directory locations from an identifier.
//!
//! ## Canonical UUID form
//! - Length: 32
//! - Characters: `0-9` and `a-f` only
//! - Example: `550e8400e29b41d4a716446655440000`
//!
//! Notes:
//! - This is the same value you would get from `Uuid::new_v4().simple().to_string()`.
//! - Canonical form is *required* for externally supplied identifiers (for example, from CLI/API
//!   inputs). Use [`UuidService::parse`] to validate an input string.
//! - Non-canonical values (uppercase, hyphenated, wrong length, non-hex) are rejected.
//!
//! ## Sharded directory layout
//! For a canonical UUID `u`, VPR stores data under:
//! `parent_dir/<u[0..2]>/<u[2..4]>/<u>/`
//!
//! Example:
//! `patient_data/clinical/55/0e/550e8400e29b41d4a716446655440000/`
//!
//! This scheme prevents very large fan-out in a single directory.
//!
//! ## Benefits of sharding
//!
//! - **Performance**: Limits directory size to prevent filesystem slowdowns
//! - **Backup efficiency**: Allows incremental backups of specific shards
//! - **Load distribution**: Spreads I/O across multiple directories
//! - **Scalability**: Supports millions of patient records without performance degradation

use crate::error::{PatientError, PatientResult};
use std::path::{Path, PathBuf};
use std::{fmt, str::FromStr};

/// Re-exported for convenience within `vpr-core`.
pub(crate) use ::uuid::Uuid;

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
/// [`UuidService::parse`] returns [`PatientError::InvalidInput`] if the input is not already
/// canonical.
///
/// # Display format
/// When displayed or converted to string, `UuidService` always produces the canonical
/// 32-character lowercase hex format without hyphens.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct UuidService(Uuid);

impl UuidService {
    /// Generates a new UUID in VPR's canonical form.
    ///
    /// This is suitable for allocating a fresh identifier during patient creation.
    /// The generated UUID is cryptographically secure and follows RFC 4122 version 4.
    ///
    /// # Returns
    ///
    /// Returns a newly generated canonical UUID wrapped in `UuidService`.
    pub(crate) fn new() -> Self {
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
    /// Returns [`PatientError::InvalidInput`] if `input` is not in canonical form.
    pub(crate) fn parse(input: &str) -> PatientResult<Self> {
        if Self::is_canonical(input) {
            // SAFETY: is_canonical guarantees valid hex, so parse_str will succeed
            let uuid = Uuid::parse_str(input).expect("is_canonical guarantees valid UUID");
            return Ok(Self(uuid));
        }
        Err(PatientError::InvalidInput(format!(
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
    pub(crate) fn uuid(&self) -> Uuid {
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
    pub(crate) fn is_canonical(input: &str) -> bool {
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
    pub(crate) fn sharded_dir(&self, parent_dir: &Path) -> PathBuf {
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
    type Err = PatientError;

    /// Parses a string into a `UuidService`, requiring canonical form.
    ///
    /// This is equivalent to calling [`UuidService::parse`].
    ///
    /// # Errors
    ///
    /// Returns [`PatientError::InvalidInput`] if the string is not in canonical UUID form.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        UuidService::parse(s)
    }
}
