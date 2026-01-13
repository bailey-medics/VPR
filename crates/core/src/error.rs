//! Error types for the VPR (Virtual Patient Record) system.
//!
//! This module defines the comprehensive error handling for all operations in the VPR core crate.
//! The [`PatientError`] enum encompasses all possible failure modes across the system's functionality,
//! including patient data management, Git versioning, cryptographic operations, and external integrations.
//!
//! # Error Categories
//!
//! Errors are organized into logical categories:
//!
//! - **Input Validation**: Invalid user inputs, malformed data, or constraint violations
//! - **File System Operations**: Directory creation, file I/O, and storage management
//! - **Serialization**: JSON and YAML encoding/decoding failures
//! - **Git Operations**: Repository management, commits, signatures, and version control
//! - **Cryptographic Operations**: ECDSA signing, key parsing, certificate validation
//! - **Author Validation**: Commit author metadata and registration verification
//! - **External Integrations**: OpenEHR system interactions
//!
//! # Error Handling Philosophy
//!
//! VPR follows defensive programming principles with comprehensive error handling:
//!
//! - **Fail Fast**: Invalid inputs and configuration are rejected early
//! - **Detailed Diagnostics**: Errors include context and source information where possible
//! - **Recovery Guidance**: Error messages are designed to be actionable for developers and operators
//! - **Type Safety**: The [`PatientResult`] type alias provides consistent error propagation
//!
//! # Usage
//!
//! Most VPR operations return [`PatientResult<T>`] to indicate success or failure:
//!
//! ```rust,ignore
//! use vpr_core::PatientResult;
//!
//! fn some_operation() -> PatientResult<String> {
//!     // Operation that might fail
//!     Ok("success".to_string())
//! }
//! ```
//!
//! Errors can be handled using standard Rust error handling patterns:
//!
//! ```rust,ignore
//! match some_operation() {
//!     Ok(result) => println!("Success: {}", result),
//!     Err(PatientError::InvalidInput(msg)) => eprintln!("Invalid input: {}", msg),
//!     Err(other) => eprintln!("Other error: {}", other),
//! }
//! ```

#[allow(clippy::single_component_path_imports)]
use serde_yaml;

/// Comprehensive error type for all VPR operations.
///
/// This enum represents all possible failure modes in the VPR system, from basic I/O operations
/// to complex cryptographic validation. Each variant includes relevant context and follows
/// consistent naming and documentation patterns.
///
/// The error messages are designed to be informative for both developers debugging issues
/// and operators maintaining production systems.
#[derive(Debug, thiserror::Error)]
pub enum PatientError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("failed to create storage directory: {0}")]
    StorageDirCreation(std::io::Error),
    #[error("failed to create patient directory: {0}")]
    PatientDirCreation(std::io::Error),
    #[error(
        "initialise failed and cleanup also failed (path: {path}): init={init_error}; cleanup={cleanup_error}",
        path = path.display()
    )]
    CleanupAfterInitialiseFailed {
        path: std::path::PathBuf,
        #[source]
        init_error: Box<PatientError>,
        cleanup_error: std::io::Error,
    },
    #[error("failed to write patient file: {0}")]
    FileWrite(std::io::Error),
    #[error("failed to read patient file: {0}")]
    FileRead(std::io::Error),
    #[error("failed to serialize patient: {0}")]
    Serialization(serde_json::Error),
    #[error("failed to deserialize patient: {0}")]
    Deserialization(serde_json::Error),
    #[error("failed to serialize YAML: {0}")]
    YamlSerialization(serde_yaml::Error),
    #[error("failed to deserialize YAML: {0}")]
    YamlDeserialization(serde_yaml::Error),

    #[error("openEHR error: {0}")]
    Openehr(#[from] openehr::OpenEhrError),
    #[error("UUID error: {0}")]
    Uuid(#[from] vpr_uuid::UuidError),
    #[error("failed to initialise git repository: {0}")]
    GitInit(git2::Error),
    #[error("failed to access git index: {0}")]
    GitIndex(git2::Error),
    #[error("failed to add file to git index: {0}")]
    GitAdd(git2::Error),
    #[error("failed to write git tree: {0}")]
    GitWriteTree(git2::Error),
    #[error("failed to find git tree: {0}")]
    GitFindTree(git2::Error),
    #[error("failed to create git signature: {0}")]
    GitSignature(git2::Error),
    #[error("failed to create initial git commit: {0}")]
    GitCommit(git2::Error),
    #[error("failed to parse PEM: {0}")]
    PemParse(::pem::PemError),
    #[error("failed to parse ECDSA private key: {0}")]
    EcdsaPrivateKeyParse(Box<dyn std::error::Error + Send + Sync>),
    #[error("failed to parse ECDSA public key/certificate: {0}")]
    EcdsaPublicKeyParse(Box<dyn std::error::Error + Send + Sync>),
    #[error("author certificate public key does not match signing key")]
    AuthorCertificatePublicKeyMismatch,
    #[error("invalid embedded commit signature payload")]
    InvalidCommitSignaturePayload,
    #[error("failed to sign: {0}")]
    EcdsaSign(Box<dyn std::error::Error + Send + Sync>),
    #[error("failed to create commit buffer: {0}")]
    GitCommitBuffer(git2::Error),
    #[error("failed to create signed commit: {0}")]
    GitCommitSigned(git2::Error),
    #[error("failed to convert commit buffer to string: {0}")]
    CommitBufferToString(std::string::FromUtf8Error),
    #[error("failed to open git repository: {0}")]
    GitOpen(git2::Error),
    #[error("failed to create/update git reference: {0}")]
    GitReference(git2::Error),
    #[error("failed to get git head: {0}")]
    GitHead(git2::Error),
    #[error("failed to set git head: {0}")]
    GitSetHead(git2::Error),
    #[error("failed to peel git commit: {0}")]
    GitPeel(git2::Error),
    #[error("invalid timestamp")]
    InvalidTimestamp,

    #[error("missing Author-Name")]
    MissingAuthorName,
    #[error("missing Author-Role")]
    MissingAuthorRole,
    #[error("invalid Author-Registration")]
    InvalidAuthorRegistration,
    #[error("author trailer keys are reserved")]
    ReservedAuthorTrailerKey,

    #[error("invalid Care-Location")]
    InvalidCareLocation,
    #[error("missing Care-Location")]
    MissingCareLocation,
    #[error("Care-Location trailer key is reserved")]
    ReservedCareLocationTrailerKey,
}

/// Type alias for Results that can fail with [`PatientError`].
///
/// This is the standard return type for all VPR operations that may fail.
/// Using this alias ensures consistent error handling across the codebase.
pub type PatientResult<T> = std::result::Result<T, PatientError>;
