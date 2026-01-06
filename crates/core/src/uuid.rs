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

use crate::{PatientError, PatientResult};
use std::path::{Path, PathBuf};
use std::{fmt, str::FromStr};

/// Re-exported for convenience within `vpr-core`.
pub(crate) use ::uuid::Uuid;

/// VPR's canonical UUID representation (32 lowercase hex characters, no hyphens).
///
/// # When to use this type
/// Use this wrapper whenever you are:
/// - Accepting a UUID string from *outside* the core (CLI input, API request, etc), or
/// - Deriving a sharded storage path for a patient.
///
/// Once you have a `UuidService`, you can safely assume the internal string is canonical.
///
/// # Construction
/// - [`UuidService::new`] generates a new canonical UUID (for new patient records).
/// - [`UuidService::parse`] validates an externally supplied identifier.
///
/// # Errors
/// [`UuidService::parse`] returns [`PatientError::InvalidInput`] if the input is not already
/// canonical.
///
/// ```rust,ignore
/// // Hyphenated or uppercase strings are rejected:
/// // UuidService::parse("550E8400-E29B-41D4-A716-446655440000")?;
/// // UuidService::parse("550e8400-e29b-41d4-a716-446655440000")?;
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct UuidService(String);

impl UuidService {
    /// Generates a new UUID in VPR's canonical form.
    ///
    /// This is suitable for allocating a fresh identifier during patient creation.
    ///
    /// # Returns
    ///
    /// Returns a newly generated canonical UUID.
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4().simple().to_string())
    }

    /// Validates and parses a UUID string that must already be in VPR's canonical form.
    ///
    /// This does **not** normalise other common UUID forms (for example, hyphenated or uppercase).
    /// Callers must provide the canonical representation.
    ///
    /// # Arguments
    ///
    /// * `input` - UUID string to validate and wrap.
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
            return Ok(Self(input.to_string()));
        }
        Err(PatientError::InvalidInput(format!(
            "UUID must be 32 lowercase hex characters without hyphens, got: '{}'",
            input
        )))
    }

    /// Returns the canonical UUID string, consuming `self`.
    ///
    /// Prefer [`std::fmt::Display`] when you only need a borrowed view.
    ///
    /// # Returns
    ///
    /// Returns the inner canonical UUID string.
    pub(crate) fn into_string(self) -> String {
        self.0
    }

    /// Returns the UUID as a `uuid::Uuid`.
    ///
    /// `UuidService` guarantees that the stored string is in canonical UUID form, so this
    /// conversion should be infallible.
    pub(crate) fn uuid(&self) -> Uuid {
        Uuid::parse_str(&self.0).expect("UuidService invariant violated: stored UUID is invalid")
    }

    /// Returns true if `input` is in VPR's canonical UUID form.
    ///
    /// This is a purely syntactic check:
    /// - 32 bytes long
    /// - lowercase hex only (`0-9` / `a-f`)
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
    /// `s1` is the first two hex characters, `s2` is the next two.
    ///
    /// # Arguments
    ///
    /// * `parent_dir` - Base directory under which to shard the UUID.
    ///
    /// # Returns
    ///
    /// Returns the fully qualified sharded directory path for this UUID.
    pub(crate) fn sharded_dir(&self, parent_dir: &Path) -> PathBuf {
        let s1 = &self.0[0..2];
        let s2 = &self.0[2..4];
        parent_dir.join(s1).join(s2).join(&self.0)
    }
}

impl fmt::Display for UuidService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for UuidService {
    type Err = PatientError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        UuidService::parse(s)
    }
}
