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

mod service;

// Re-export public types
pub use service::{TimestampId, TimestampIdGenerator, Uuid, UuidService};

/// Error type for UUID operations.
#[derive(Debug, thiserror::Error)]
pub enum UuidError {
    /// Invalid input provided
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

/// Result type for UUID operations.
pub type UuidResult<T> = Result<T, UuidError>;
