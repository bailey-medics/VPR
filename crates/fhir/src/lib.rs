//! FHIR wire/boundary support for Care Coordination Repository.
//!
//! This crate provides **wire models** and **format/translation helpers** for on-disk,
//! version-controlled coordination files:
//! - YAML components (for example messaging thread ledgers)
//!
//! This crate focuses on:
//! - FHIR semantic alignment (without FHIR JSON/REST transport)
//! - serialisation/deserialisation
//! - translation between domain primitives and wire structs
//!
//! Unlike the openehr crate, this crate is NOT version-aware. FHIR-aligned structures
//! evolve more slowly and are internally versioned when needed.

pub mod coordination_status;
pub mod messaging;
pub mod patient;

// Re-export facades
pub use coordination_status::CoordinationStatus;
pub use messaging::Messaging;
pub use patient::Patient;

// Re-export public domain-level types
pub use coordination_status::{CoordinationStatusData, LifecycleState};
pub use messaging::{AuthorRole, LedgerData, MessageAuthor, SensitivityLevel, ThreadStatus};
pub use patient::{NameUse, PatientData};

// Re-export TimestampId from vpr_uuid crate
pub use vpr_uuid::TimestampId;

/// Errors returned by the `fhir` boundary crate.
#[derive(Debug, thiserror::Error)]
pub enum FhirError {
    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("invalid YAML: {0}")]
    InvalidYaml(#[from] serde_yaml::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("translation error: {0}")]
    Translation(String),

    #[error("invalid UUID: {0}")]
    InvalidUuid(String),
}

/// Type alias for Results that can fail with a [`FhirError`].
pub type FhirResult<T> = Result<T, FhirError>;
