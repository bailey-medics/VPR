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
pub mod git;
pub mod repositories;
pub(crate) mod uuid;

pub mod error;

pub mod patient;

// Use the shared api-shared crate for generated protobuf types.
pub use api_shared::pb;

// Re-export commonly used constants
pub use constants::DEFAULT_PATIENT_DATA_DIR;

pub use config::CoreConfig;

// TODO: need to check if all of these re-exports are necessary
// Re-export author types
pub use author::{
    extract_embedded_commit_signature, Author, AuthorRegistration, EmbeddedCommitSignature,
};

// Re-export repo utilities
pub use repositories::helpers::{add_directory_to_index, copy_dir_recursive};

// Re-export error types
pub use error::{PatientError, PatientResult};

// Re-export patient types
pub use patient::{FullRecord, PatientService};
