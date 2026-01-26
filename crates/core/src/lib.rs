//! # VPR Core
//!
//! Core business logic for the VPR patient record system.
//!
//! This crate contains pure data operations and file/folder management:
//! - Patient creation and listing with sharded JSON storage
//! - File system operations under the configured patient data directory
//! - Git-like versioning
//!
//! **No API concerns**: Authentication, HTTP/gRPC servers, or service interfaces belong in `api-grpc`, `api-rest`, or `api-shared`.

pub mod author;
pub mod config;
pub mod constants;
pub mod markdown;
pub mod paths;
pub mod repositories;
pub mod versioned_files;

pub mod error;

pub mod patient;

pub use config::CoreConfig;

// Re-export commonly used constants
pub use constants::DEFAULT_PATIENT_DATA_DIR;

// Re-export author types
pub use author::{Author, AuthorRegistration};

// Re-export patient types
pub use patient::PatientService;

// Re-export UUID types from vpr-uuid crate
pub use vpr_uuid::{ShardableUuid, TimestampId, TimestampIdGenerator, Uuid};

// Re-export types from vpr-types crate
pub use vpr_types::{NonEmptyText, TextError};
