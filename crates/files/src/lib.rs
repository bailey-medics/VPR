//! VPR File Storage
//!
//! This crate provides file storage and management for the Versioned Patient Repository (VPR).
//!
//! ## Design Principles
//!
//! Following the VPR file storage specification:
//!
//! - Semantic meaning and binary bytes are deliberately separated
//! - Binary files are not tracked in Git
//! - Binary files are immutable once added (new content creates a new file)
//! - References to files are explicit, auditable, and versioned
//! - Repositories remain valid even when binary files are absent
//! - No global or cross-repository binary namespace exists
//!
//! ## Repository-Scoped Storage Model
//!
//! Each repository is self-contained and stores its own associated files alongside
//! its versioned content. Works with all repository types (CR, DR, CCR, RRR):
//!
//! ```text
//! <repository_type>/
//! └── <repository_id>/
//!     ├── .gitignore
//!     ├── <type-specific-dirs>/
//!     └── files/        # gitignored
//!         └── sha256/
//!             └── ab/
//!                 └── ab3f9e…
//! ```
//!
//! ## Example Usage
//!
//! ```no_run
//! use vpr_files::FilesService;
//! use vpr_uuid::ShardableUuid;
//! use std::path::Path;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Works with any repository type (clinical, demographics, coordination, research)
//! let root = Path::new("patient_data/clinical");
//! let repository_id = ShardableUuid::parse("550e8400e29b41d4a716446655440000")?;
//!
//! let service = FilesService::new(root, repository_id)?;
//! # Ok(())
//! # }
//! ```

mod constants;
mod files;

pub use constants::FILES_FOLDER_NAME;
pub use files::{FileMetadata, FilesService};
pub use vpr_uuid::ShardableUuid;

/// Errors that can occur during file operations
#[derive(Debug, thiserror::Error)]
pub enum FilesError {
    /// Root directory does not exist or is not a directory
    #[error("Invalid root directory: {0}")]
    InvalidRootDirectory(String),

    /// Repository directory does not exist
    #[error("Repository not found: {0}")]
    RepositoryNotFound(String),

    /// Path validation failed (potential directory traversal or unsafe path)
    #[error("Invalid path: {0}")]
    InvalidPath(String),

    /// File already exists in content-addressed storage (immutability violation)
    #[error("File with hash {0} already exists in storage")]
    FileAlreadyExists(String),

    /// I/O error occurred
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// UUID error from vpr-uuid crate
    #[error("UUID error: {0}")]
    Uuid(#[from] vpr_uuid::UuidError),
}
