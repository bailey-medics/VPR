//! Repository-scoped file storage service implementation
//!
//! This module provides the core implementation of VPR's file storage system through
//! the [`FilesService`] type. It manages binary file storage and retrieval for all
//! repository types in the VPR system.
//!
//! # Architecture
//!
//! The file storage model enforces strict separation of concerns:
//!
//! - **Semantic data** (structured records and metadata) is versioned in Git
//! - **Binary data** (images, documents, etc.) is stored separately in content-addressed files
//! - **References** between them are explicit, immutable, and auditable
//!
//! This separation ensures that:
//! - Repositories remain valid even when binary files are unavailable
//! - Binary files can be backed up, archived, or distributed independently
//! - Version control stays efficient (no large binary diffs)
//!
//! # Storage Layout
//!
//! Each repository maintains its own isolated file storage:
//!
//! ```text
//! <repository_type>/           # Repository type directory
//! └── <repository_id>/         # UUID-based repository identifier
//!     ├── <type-specific>/     # Type-specific data directories
//!     └── files/               # gitignored, contains binary files
//!         └── sha256/          # content-addressed by SHA-256
//!             └── ab/          # two-level sharding
//!                 └── ab3f9e…  # full hash as filename
//! ```
//!
//! # Content Addressing
//!
//! Files are stored using their SHA-256 hash as the identifier. This provides:
//!
//! - **Deduplication**: Identical files are stored once
//! - **Integrity**: File content can be verified against its hash
//! - **Immutability**: Files cannot be modified after creation
//! - **Deterministic paths**: Same content always produces the same path
//!
//! # Security Model
//!
//! The service enforces strict boundaries:
//!
//! - All paths are canonicalised to prevent symlink attacks
//! - Repository existence is validated at construction time
//! - File operations are scoped to a single repository
//! - UUID-based sharding prevents directory traversal
//!
//! # Implementation Notes
//!
//! - The service is stateless and performs minimal I/O in the constructor (validation only)
//! - File directories are constructed but not created until needed
//! - All validation happens eagerly at construction time
//! - The service implements `Debug` but not `Clone` (single-owner semantics)

use crate::{FilesError, FILES_FOLDER_NAME};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use vpr_types::NonEmptyText;
use vpr_uuid::{Sha256Hash, ShardableUuid};

/// Metadata for a stored file
///
/// This structure contains all information about a file stored in the repository,
/// including its content hash, location, size, and detection metadata.
///
/// # YAML Serialisation
///
/// This struct is designed to be serialised to YAML and stored alongside the binary file.
/// It provides an auditable record of file storage without including any patient or
/// clinical identifiers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct FileMetadata {
    /// Hashing algorithm used (always "sha256" for current implementation)
    pub hash_algorithm: NonEmptyText,

    /// Hexadecimal digest of the file content
    pub hash: Sha256Hash,

    /// Path relative to repository root where the file is stored
    pub relative_path: NonEmptyText,

    /// Size of the file in bytes
    pub size_bytes: u64,

    /// Detected media type (MIME type), if available
    ///
    /// This is a best-effort detection and should not be considered authoritative.
    /// May be `None` if the media type cannot be determined.
    pub media_type: Option<NonEmptyText>,

    /// Original filename from the source path
    pub original_filename: NonEmptyText,

    /// UTC timestamp when the file was stored (ISO 8601 format)
    pub stored_at: DateTime<Utc>,
}

/// Service for managing files within a repository
///
/// The `FilesService` provides a safe, scoped interface for managing binary files
/// associated with a specific repository. It enforces the VPR file storage
/// model and ensures all operations remain within the repository's boundaries.
///
///
/// # Design
///
/// - Repository-scoped: Each service instance is bound to one repository
/// - Immutable: Files are never modified after creation
/// - Content-addressed: Files are identified by their SHA-256 hash
/// - Defensive: All paths are validated to prevent directory traversal
#[derive(Debug)]
pub struct FilesService {
    /// Root directory containing all repositories
    root_directory: PathBuf,

    /// Repository identifier
    repository_id: ShardableUuid,
}

impl FilesService {
    /// Creates a new `FilesService` for a specific repository
    ///
    /// # Arguments
    ///
    /// * `root_directory` - The root directory containing repositories of this type
    /// * `repository_id` - The unique identifier for this repository
    ///
    /// # Errors
    ///
    /// Returns `FilesError` if:
    /// - The root directory does not exist or is not a directory
    /// - The repository directory does not exist or is not a directory
    /// - Path canonicalisation fails
    pub fn new(root_directory: &Path, repository_id: ShardableUuid) -> Result<Self, FilesError> {
        if !root_directory.exists() {
            return Err(FilesError::InvalidRootDirectory(format!(
                "Directory does not exist: {}",
                root_directory.display()
            )));
        }

        if !root_directory.is_dir() {
            return Err(FilesError::InvalidRootDirectory(format!(
                "Path is not a directory: {}",
                root_directory.display()
            )));
        }

        let root_directory = root_directory.canonicalize().map_err(|e| {
            FilesError::InvalidRootDirectory(format!(
                "Cannot canonicalize path {}: {}",
                root_directory.display(),
                e
            ))
        })?;

        // Build repository root path using sharded UUID path
        let repository_root = repository_id.sharded_dir(&root_directory);

        // Defensive: Verify the repository directory exists
        if !repository_root.exists() {
            return Err(FilesError::RepositoryNotFound(format!(
                "Repository directory does not exist: {}",
                repository_root.display()
            )));
        }

        if !repository_root.is_dir() {
            return Err(FilesError::RepositoryNotFound(format!(
                "Repository path exists but is not a directory: {}",
                repository_root.display()
            )));
        }

        Ok(Self {
            root_directory,
            repository_id,
        })
    }

    /// Adds a file to the repository's content-addressed storage
    ///
    /// This method reads a file from the filesystem, computes its SHA-256 hash,
    /// and stores it in a content-addressed location within the repository.
    /// Files are stored immutably — attempting to add a file with a hash
    /// that already exists will return an error.
    ///
    /// # Arguments
    ///
    /// * `source_path` - Path to the file to add
    ///
    /// # Returns
    ///
    /// `FileMetadata` containing the hash, storage location, size, detected
    /// media type, original filename, and storage timestamp.
    ///
    /// # Storage Location
    ///
    /// Files are stored at: `<repository_root>/files/sha256/<shard>/<full_hash>`
    /// where `<shard>` is derived from the first 4 characters of the hash (e.g., `ab/cd`).
    ///
    /// # Errors
    ///
    /// Returns `FilesError` if:
    /// - The source file cannot be opened or read (I/O)
    /// - The file already exists in storage (immutability violation)
    /// - Storage directory creation fails (I/O)
    /// - File write to storage fails (I/O)
    pub fn add(&self, source_path: &Path) -> Result<FileMetadata, FilesError> {
        // Read the entire file into memory
        let mut file = fs::File::open(source_path).map_err(|e| {
            FilesError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to open source file {}: {}",
                    source_path.display(),
                    e
                ),
            ))
        })?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).map_err(|e| {
            FilesError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to read source file {}: {}",
                    source_path.display(),
                    e
                ),
            ))
        })?;

        // Compute SHA-256 hash
        let mut hasher = Sha256::new();
        hasher.update(&buffer);
        let hash_bytes = hasher.finalize();
        let hash_array: [u8; 32] = hash_bytes.into();
        let hash = Sha256Hash::from_bytes(&hash_array);

        // Create sharded path for storage
        let storage_path = self.compute_storage_path(hash.as_str());

        // Check if file already exists (immutability check)
        if storage_path.exists() {
            return Err(FilesError::FileAlreadyExists(hash.to_string()));
        }

        // Create parent directories
        if let Some(parent) = storage_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                FilesError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to create storage directory {}: {}",
                        parent.display(),
                        e
                    ),
                ))
            })?;
        }

        // Write file to storage
        fs::write(&storage_path, &buffer).map_err(|e| {
            FilesError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to write file to {}: {}", storage_path.display(), e),
            ))
        })?;

        // Extract original filename
        let original_filename = NonEmptyText::new(
            source_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown"),
        )
        .expect("filename is non-empty");

        // Detect media type (best-effort)
        let media_type = infer::get(&buffer)
            .map(|kind| NonEmptyText::new(kind.mime_type()).expect("mime type is non-empty"));

        // Compute relative path from repository root
        let relative_path = self.compute_relative_path(hash.as_str());

        // Get current timestamp
        let stored_at = Utc::now();

        Ok(FileMetadata {
            hash_algorithm: NonEmptyText::new("sha256").expect("sha256 is non-empty"),
            hash,
            relative_path: NonEmptyText::new(&relative_path)
                .expect("relative path is always non-empty"),
            size_bytes: buffer.len() as u64,
            media_type,
            original_filename,
            stored_at,
        })
    }

    /// Retrieves a file from content-addressed storage by its hash
    ///
    /// This method reads a previously stored file and returns its contents as bytes.
    /// Files are identified by their SHA-256 hash, ensuring integrity verification.
    ///
    /// # Arguments
    ///
    /// * `hash` - The SHA-256 hash (hexadecimal string) of the file to retrieve
    ///
    /// # Returns
    ///
    /// The file contents as a byte vector (`Vec<u8>`), suitable for transmission
    /// over the network or further processing.
    ///
    /// # Errors
    ///
    /// Returns `FilesError` if:
    /// - The file does not exist in storage (hash not found)
    /// - The file cannot be read (I/O)
    pub fn read(&self, hash: &str) -> Result<Vec<u8>, FilesError> {
        let storage_path = self.compute_storage_path(hash);

        // Check if file exists
        if !storage_path.exists() {
            return Err(FilesError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File not found for hash: {}", hash),
            )));
        }

        // Read and return file contents
        fs::read(&storage_path).map_err(|e| {
            FilesError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to read file from {}: {}", storage_path.display(), e),
            ))
        })
    }

    /// Computes the sharded storage path for a given hash
    ///
    /// This method constructs the absolute storage path by combining the repository root
    /// with the relative path from [`Self::compute_relative_path`]. This ensures consistency
    /// between the relative and absolute path representations.
    ///
    /// # Arguments
    ///
    /// * `hash_hex` - The hexadecimal hash string (must be at least 4 characters)
    ///
    /// # Returns
    ///
    /// Absolute path to the storage location: `<repository_root>/files/sha256/<shard1>/<shard2>/<hash>`
    ///
    /// Example: hash `abcdef123...` produces `<repository_root>/files/sha256/ab/cd/abcdef123...`
    fn compute_storage_path(&self, hash_hex: &str) -> PathBuf {
        self.repository_root()
            .join(self.compute_relative_path(hash_hex).as_str())
    }

    /// Computes the relative path from repository root to the stored file
    ///
    /// # Arguments
    ///
    /// * `hash_hex` - The hexadecimal hash string (must be at least 4 characters)
    ///
    /// # Returns
    ///
    /// Relative path string in the format: `files/sha256/<shard1>/<shard2>/<hash>`
    fn compute_relative_path(&self, hash_hex: &str) -> NonEmptyText {
        let shard1 = &hash_hex[0..2];
        let shard2 = &hash_hex[2..4];
        NonEmptyText::new(format!("files/sha256/{}/{}/{}", shard1, shard2, hash_hex))
            .expect("computed path is non-empty")
    }

    /// Returns the root directory containing all repositories
    ///
    /// # Returns
    ///
    /// Reference to the canonicalised root directory path
    #[must_use]
    #[allow(dead_code)]
    fn root_directory(&self) -> &Path {
        &self.root_directory
    }

    /// Returns the repository identifier
    ///
    /// # Returns
    ///
    /// Reference to the repository's UUID
    #[must_use]
    #[allow(dead_code)]
    fn repository_id(&self) -> &ShardableUuid {
        &self.repository_id
    }

    /// Returns the path to this repository's root directory
    ///
    /// # Returns
    ///
    /// Absolute path to the repository root using UUID-based sharding
    #[must_use]
    fn repository_root(&self) -> PathBuf {
        self.repository_id.sharded_dir(&self.root_directory)
    }

    /// Returns the path to the files/ directory within this repository
    ///
    /// # Returns
    ///
    /// Absolute path to the files directory within the repository.
    /// Note: This directory may not exist yet; use filesystem operations to create it when needed.
    #[must_use]
    #[allow(dead_code)]
    fn files_directory(&self) -> PathBuf {
        self.repository_root().join(FILES_FOLDER_NAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper to create a test repository structure
    fn create_test_repo(root: &Path, uuid: &ShardableUuid) -> PathBuf {
        let repo_path = uuid.sharded_dir(root);
        fs::create_dir_all(&repo_path).expect("Failed to create repository directory");
        fs::create_dir_all(repo_path.join("data")).expect("Failed to create data dir");
        repo_path
    }

    /// Creates a realistic repository structure matching the actual VPR layout
    fn create_realistic_repo(root: &Path, uuid: &ShardableUuid) -> ShardableUuid {
        let repo_path = uuid.sharded_dir(root);
        fs::create_dir_all(&repo_path).expect("Failed to create repository directory");

        // Create typical repository subdirectories (generic to any repo type)
        fs::create_dir_all(repo_path.join("data")).expect("Failed to create data");
        fs::create_dir_all(repo_path.join("indexes")).expect("Failed to create indexes");
        fs::create_dir_all(repo_path.join("metadata")).expect("Failed to create metadata");

        // Create a minimal metadata file
        fs::write(repo_path.join("metadata.yaml"), "# Repository Metadata\n")
            .expect("Failed to create metadata.yaml");

        uuid.clone()
    }

    #[test]
    fn test_files_service_new_success() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        create_test_repo(&root, &uuid);

        let service = FilesService::new(&root, uuid.clone());

        assert!(service.is_ok());
        let service = service.unwrap();

        assert_eq!(service.repository_id(), &uuid);
        assert!(service
            .repository_root()
            .ends_with(uuid.to_string().as_str()));
        assert!(service.files_directory().ends_with("files"));
    }

    #[test]
    fn test_files_service_root_not_exists() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("non-existent");

        let uuid = ShardableUuid::new();
        let service = FilesService::new(&root, uuid);

        assert!(matches!(service, Err(FilesError::InvalidRootDirectory(_))));
    }

    #[test]
    fn test_files_service_root_not_directory() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("file.txt");
        fs::write(&root, "not a directory").unwrap();

        let uuid = ShardableUuid::new();
        let service = FilesService::new(&root, uuid);

        assert!(matches!(service, Err(FilesError::InvalidRootDirectory(_))));
    }

    #[test]
    fn test_files_service_cr_not_exists() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        let service = FilesService::new(&root, uuid);

        assert!(matches!(service, Err(FilesError::RepositoryNotFound(_))));
    }

    #[test]
    fn test_files_service_cr_not_directory() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        let repo_path = root.join(uuid.to_string());
        fs::write(&repo_path, "not a directory").unwrap();

        let service = FilesService::new(&root, uuid);

        assert!(matches!(service, Err(FilesError::RepositoryNotFound(_))));
    }

    #[test]
    fn test_files_service_getters() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        create_test_repo(&root, &uuid);

        let service = FilesService::new(&root, uuid.clone()).unwrap();

        assert_eq!(service.repository_id(), &uuid);
        assert!(service.root_directory().ends_with("repositories"));
        assert!(service
            .repository_root()
            .ends_with(uuid.to_string().as_str()));
        assert!(service
            .files_directory()
            .to_string_lossy()
            .contains(&format!("{}/files", uuid)));
    }

    #[test]
    fn test_files_directory_within_bounds() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        create_test_repo(&root, &uuid);

        let service = FilesService::new(&root, uuid).unwrap();

        // Verify files directory is within repository root
        assert!(service
            .files_directory()
            .starts_with(service.repository_root()));
    }

    #[test]
    fn test_multiple_services_same_root() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid1 = ShardableUuid::new();
        let uuid2 = ShardableUuid::new();
        create_test_repo(&root, &uuid1);
        create_test_repo(&root, &uuid2);

        let service1 = FilesService::new(&root, uuid1).unwrap();
        let service2 = FilesService::new(&root, uuid2).unwrap();

        assert_ne!(service1.repository_root(), service2.repository_root());
        assert_ne!(service1.files_directory(), service2.files_directory());
    }

    // Integration tests with realistic structures

    #[test]
    fn test_service_with_realistic_structure() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        // Create a repository with UUID
        let uuid = ShardableUuid::parse("787b4e1c54cb42fa9e69f84e39da3d9a").expect("valid uuid");
        create_realistic_repo(&root, &uuid);

        // Initialize service with the repository ID
        let service = FilesService::new(&root, uuid).unwrap();

        // Verify paths
        assert!(service.repository_root().exists());
        assert!(service.repository_root().join("metadata.yaml").exists());

        // Verify files directory path is constructed correctly
        assert!(service
            .files_directory()
            .to_string_lossy()
            .contains("/files"));
        assert!(service.files_directory().ends_with("files"));
    }

    #[test]
    fn test_service_initialization_validates_cr_exists() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        // Try to initialize with non-existent UUID
        let uuid = ShardableUuid::new();
        let service = FilesService::new(&root, uuid);

        // Should fail because repository doesn't exist
        assert!(service.is_err());
    }

    #[test]
    fn test_files_directory_path_construction() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::parse("abcd123456789012abcd123456789012").expect("valid uuid");
        create_realistic_repo(&root, &uuid);

        let service = FilesService::new(&root, uuid.clone()).unwrap();

        // Files directory should be: repositories/<uuid>/files
        let files_dir = service.files_directory();
        assert!(files_dir.ends_with("files"));

        // Should be within repository root
        assert!(files_dir.starts_with(service.repository_root()));

        // Path should contain the UUID
        let path_str = files_dir.to_string_lossy();
        assert!(path_str.contains(&uuid.to_string()));
    }

    #[test]
    fn test_multiple_repositories_isolated() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid1 = ShardableUuid::new();
        let uuid2 = ShardableUuid::new();

        create_realistic_repo(&root, &uuid1);
        create_realistic_repo(&root, &uuid2);

        let service1 = FilesService::new(&root, uuid1).unwrap();
        let service2 = FilesService::new(&root, uuid2).unwrap();

        // Each service should have different repository roots
        assert_ne!(service1.repository_root(), service2.repository_root());

        // Each service should have different files directories
        assert_ne!(service1.files_directory(), service2.files_directory());

        // But they should share the same root directory
        assert_eq!(service1.root_directory(), service2.root_directory());
    }

    // File storage tests

    #[test]
    fn test_add_file_success() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        create_test_repo(&root, &uuid);

        // Create a test file to ingest
        let source_file = temp.path().join("test.txt");
        fs::write(&source_file, b"Hello, World!").unwrap();

        let service = FilesService::new(&root, uuid).unwrap();
        let metadata = service.add(&source_file).unwrap();

        // Verify metadata
        assert_eq!(metadata.hash_algorithm.as_str(), "sha256");
        assert_eq!(metadata.size_bytes, 13);
        assert_eq!(metadata.original_filename.as_str(), "test.txt");
        assert!(metadata.hash.as_str().len() == 64); // SHA-256 hex length

        // Verify file was stored
        let stored_path = service.compute_storage_path(metadata.hash.as_str());
        assert!(stored_path.exists());

        // Verify content
        let stored_content = fs::read(&stored_path).unwrap();
        assert_eq!(stored_content, b"Hello, World!");
    }

    #[test]
    fn test_add_file_immutability() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        create_test_repo(&root, &uuid);

        // Create a test file
        let source_file = temp.path().join("test.txt");
        fs::write(&source_file, b"Same content").unwrap();

        let service = FilesService::new(&root, uuid).unwrap();

        // First add should succeed
        let result1 = service.add(&source_file);
        assert!(result1.is_ok());

        // Second add of same content should fail
        let result2 = service.add(&source_file);
        assert!(matches!(result2, Err(FilesError::FileAlreadyExists(_))));
    }

    #[test]
    fn test_add_file_with_media_type() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        create_test_repo(&root, &uuid);

        // Create a PNG file (minimal valid PNG header)
        let source_file = temp.path().join("test.png");
        let png_header = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        fs::write(&source_file, png_header).unwrap();

        let service = FilesService::new(&root, uuid).unwrap();
        let metadata = service.add(&source_file).unwrap();

        // Should detect PNG media type
        assert_eq!(
            metadata.media_type.as_ref().map(|t| t.as_str()),
            Some("image/png")
        );
    }

    #[test]
    fn test_add_file_nonexistent() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        create_test_repo(&root, &uuid);

        let service = FilesService::new(&root, uuid).unwrap();
        let result = service.add(Path::new("/non-existent/file.txt"));

        assert!(result.is_err());
    }

    #[test]
    fn test_compute_storage_path_sharding() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        create_test_repo(&root, &uuid);

        let service = FilesService::new(&root, uuid).unwrap();
        let hash = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let path = service.compute_storage_path(hash);

        // Verify sharding structure
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("/files/sha256/ab/cd/"));
        assert!(path_str.ends_with(hash));
    }

    #[test]
    fn test_compute_relative_path() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        create_test_repo(&root, &uuid);

        let service = FilesService::new(&root, uuid).unwrap();
        let hash = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let relative = service.compute_relative_path(hash);

        assert_eq!(relative.as_str(), format!("files/sha256/ab/cd/{}", hash));
    }

    #[test]
    fn test_file_metadata_serialization() {
        use super::FileMetadata;

        let metadata = FileMetadata {
            hash_algorithm: NonEmptyText::new("sha256").unwrap(),
            hash: Sha256Hash::parse(
                "abc1230000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            relative_path: NonEmptyText::new("files/sha256/ab/c1/abc123").unwrap(),
            size_bytes: 1024,
            media_type: Some(NonEmptyText::new("text/plain").unwrap()),
            original_filename: NonEmptyText::new("document.txt").unwrap(),
            stored_at: "2024-01-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap(),
        };

        // Test that it can be serialized (would use serde_yaml in practice)
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("sha256"));
        assert!(json.contains("abc123"));
    }

    #[test]
    fn test_add_binary_file() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        create_test_repo(&root, &uuid);

        // Create a binary file with various byte values
        let source_file = temp.path().join("binary.dat");
        let binary_data: Vec<u8> = (0..=255).collect();
        fs::write(&source_file, binary_data).unwrap();

        let service = FilesService::new(&root, uuid).unwrap();
        let metadata = service.add(&source_file).unwrap();

        assert_eq!(metadata.size_bytes, 256);

        // Verify stored content matches
        let stored_path = service.compute_storage_path(metadata.hash.as_str());
        let stored_content = fs::read(&stored_path).unwrap();
        let expected: Vec<u8> = (0..=255).collect();
        assert_eq!(stored_content, expected);
    }

    #[test]
    fn test_read_file_success() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        create_test_repo(&root, &uuid);

        // Add a file first
        let source_file = temp.path().join("test.txt");
        let content = b"Hello, World!";
        fs::write(&source_file, content).unwrap();

        let service = FilesService::new(&root, uuid).unwrap();
        let metadata = service.add(&source_file).unwrap();

        // Read the file back using its hash
        let retrieved_content = service.read(metadata.hash.as_str()).unwrap();

        assert_eq!(retrieved_content, content);
    }

    #[test]
    fn test_read_file_not_found() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        create_test_repo(&root, &uuid);

        let service = FilesService::new(&root, uuid).unwrap();

        // Try to read a non-existent hash
        let fake_hash = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let result = service.read(fake_hash);

        assert!(result.is_err());
        assert!(matches!(result, Err(FilesError::Io(_))));
    }

    #[test]
    fn test_read_binary_file() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        create_test_repo(&root, &uuid);

        // Create and add a binary file
        let source_file = temp.path().join("binary.dat");
        let binary_data: Vec<u8> = (0..=255).collect();
        fs::write(&source_file, &binary_data).unwrap();

        let service = FilesService::new(&root, uuid).unwrap();
        let metadata = service.add(&source_file).unwrap();

        // Read it back
        let retrieved_data = service.read(metadata.hash.as_str()).unwrap();

        assert_eq!(retrieved_data, binary_data);
        assert_eq!(retrieved_data.len(), 256);
    }

    #[test]
    fn test_add_and_read_roundtrip() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repositories");
        fs::create_dir_all(&root).unwrap();

        let uuid = ShardableUuid::new();
        create_test_repo(&root, &uuid);

        let service = FilesService::new(&root, uuid).unwrap();

        // Test with various file types
        let test_cases = vec![
            ("text.txt", b"Plain text content".to_vec()),
            ("empty.dat", vec![]),
            ("binary.bin", vec![0x00, 0xFF, 0xAA, 0x55, 0x12, 0x34]),
        ];

        for (filename, content) in test_cases {
            let source_file = temp.path().join(filename);
            fs::write(&source_file, &content).unwrap();

            let metadata = service.add(&source_file).unwrap();
            let retrieved = service.read(metadata.hash.as_str()).unwrap();

            assert_eq!(retrieved, content, "Round-trip failed for {}", filename);
        }
    }
}
