//! # VPR Core
//!
//! Core business logic for the VPR patient record system.
//!
//! This crate contains pure data operations and file/folder management:
//! - Patient creation and listing with sharded JSON storage
//! - File system operations under `PATIENT_DATA_DIR`
//! - Git-like versioning
//!
//! **No API concerns**: Authentication, HTTP/gRPC servers, or service interfaces belong in `api-grpc`, `api-rest`, or `api-shared`.

pub mod clinical;
pub mod constants;
pub mod demographics;
pub(crate) mod git;
pub(crate) mod uuid;

// Use the shared api-shared crate for generated protobuf types.
pub use api_shared::pb;

// Re-export commonly used constants
pub use constants::DEFAULT_PATIENT_DATA_DIR;

#[allow(clippy::single_component_path_imports)]
use serde_yaml;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Author {
    pub name: String,
    pub email: String,
    pub signature: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PatientError {
    #[error("first_name and last_name are required")]
    InvalidInput,
    #[error("failed to create storage directory: {0}")]
    StorageDirCreation(std::io::Error),
    #[error("failed to create patient directory: {0}")]
    PatientDirCreation(std::io::Error),
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
    PemParse(pem::PemError),
    #[error("failed to parse ECDSA private key: {0}")]
    EcdsaPrivateKeyParse(Box<dyn std::error::Error + Send + Sync>),
    #[error("failed to parse ECDSA public key/certificate: {0}")]
    EcdsaPublicKeyParse(Box<dyn std::error::Error + Send + Sync>),
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
}

pub type PatientResult<T> = std::result::Result<T, PatientError>;

/// Represents a complete patient record with both demographics and clinical components.
#[derive(Debug)]
pub struct FullRecord {
    /// The UUID of the demographics record.
    pub demographics_uuid: String,
    /// The UUID of the clinical record.
    pub clinical_uuid: String,
}

/// Pure patient data operations - no API concerns
#[derive(Default, Clone)]
pub struct PatientService;

impl PatientService {
    /// Creates a new instance of PatientService.
    ///
    /// # Returns
    /// A new `PatientService` instance ready to handle patient operations.
    pub fn new() -> Self {
        Self
    }

    /// Initialises a complete patient record with demographics and clinical components.
    ///
    /// This function creates both a demographics repository and a clinical repository,
    /// links them together, and populates the demographics with the provided patient information.
    ///
    /// # Arguments
    ///
    /// * `author` - The author information for Git commits.
    /// * `given_names` - A vector of the patient's given names.
    /// * `last_name` - The patient's family/last name.
    /// * `birth_date` - The patient's date of birth as a string (e.g., "YYYY-MM-DD").
    /// * `namespace` - Optional namespace for the clinical-demographics link.
    ///
    /// # Returns
    ///
    /// Returns a `FullRecord` containing both UUIDs on success.
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if any step in the initialisation fails.
    pub fn initialise_full_record(
        &self,
        author: Author,
        given_names: Vec<String>,
        last_name: String,
        birth_date: String,
        namespace: Option<String>,
    ) -> PatientResult<FullRecord> {
        let demographics_service = crate::demographics::DemographicsService;
        // Initialise demographics
        let demographics_uuid = demographics_service.initialise(author.clone())?;

        // Update demographics with patient information
        demographics_service.update(&demographics_uuid, given_names, &last_name, &birth_date)?;

        // Initialise clinical
        let clinical_service = crate::clinical::ClinicalService;
        let clinical_uuid = clinical_service.initialise(author)?;

        // Link clinical to demographics
        clinical_service.link_to_demographics(&clinical_uuid, &demographics_uuid, namespace)?;

        Ok(FullRecord {
            demographics_uuid,
            clinical_uuid,
        })
    }
}

/// YAML file management utilities
///
/// These functions provide safe operations for creating and updating YAML files
/// using serde_yaml::Value for maximum flexibility in merging and modifying data.
/// Updates a YAML file by merging new data with existing content (or creating if not exists).
///
/// This function reads the current YAML file (if it exists), merges the new data into it,
/// and writes back the result. Merging is performed as follows:
/// - If file doesn't exist: uses new_data as-is
/// - If both current and new_data are Mappings: merges new_data fields into current
/// - Otherwise: replaces current with new_data
///
/// # Arguments
/// * `path` - Path to the YAML file
/// * `new_data` - The new data to merge into the existing file
pub fn yaml_write<T: serde::Serialize>(path: &Path, new_data: &T) -> Result<(), PatientError> {
    let new_value = serde_yaml::to_value(new_data).map_err(PatientError::YamlSerialization)?;
    let current = if path.exists() {
        let yaml_str = fs::read_to_string(path).map_err(PatientError::FileRead)?;
        serde_yaml::from_str(&yaml_str).map_err(PatientError::YamlDeserialization)?
    } else {
        serde_yaml::Value::Null
    };

    let merged_value = merge_yaml_values(current, new_value);
    let yaml = serde_yaml::to_string(&merged_value).map_err(PatientError::YamlSerialization)?;
    fs::write(path, yaml).map_err(PatientError::FileWrite)
}

/// Merges two YAML values according to the yaml_write rules
fn merge_yaml_values(current: serde_yaml::Value, new_data: serde_yaml::Value) -> serde_yaml::Value {
    match (current, new_data) {
        (serde_yaml::Value::Null, new) => new,
        (serde_yaml::Value::Mapping(mut current_map), serde_yaml::Value::Mapping(new_map)) => {
            // Merge new_map into current_map
            for (key, value) in new_map {
                current_map.insert(key, value);
            }
            serde_yaml::Value::Mapping(current_map)
        }
        (_, new) => new, // Replace current with new for non-mapping cases
    }
}

/// Returns the patient data directory path from environment variable or default.
///
/// This function reads the `PATIENT_DATA_DIR` environment variable and returns
/// the path as a `PathBuf`. If the environment variable is not set, it falls
/// back to the default path defined in `constants::DEFAULT_PATIENT_DATA_DIR`.
///
/// # Returns
/// A `PathBuf` pointing to the patient data directory.
/// // TODO: try and delete (demographics and clinical use it directly)
pub fn patient_data_path() -> std::path::PathBuf {
    let base = std::env::var("PATIENT_DATA_DIR")
        .unwrap_or_else(|_| constants::DEFAULT_PATIENT_DATA_DIR.into());
    std::path::PathBuf::from(base)
}

/// Returns the clinical data directory path.
///
/// This function reads the `PATIENT_DATA_DIR` environment variable and returns
/// the path to the clinical data directory. If the environment variable is not set,
/// it falls back to the default path defined in `constants::DEFAULT_PATIENT_DATA_DIR`,
/// then appends the clinical directory name.
///
/// # Returns
/// A `PathBuf` pointing to the clinical data directory.
pub fn clinical_data_path() -> std::path::PathBuf {
    let base = std::env::var("PATIENT_DATA_DIR")
        .unwrap_or_else(|_| constants::DEFAULT_PATIENT_DATA_DIR.into());
    let data_dir = std::path::PathBuf::from(base);
    data_dir.join(constants::CLINICAL_DIR_NAME)
}

/// Recursively copies a directory and its contents to a destination.
///
/// This function creates the destination directory if it doesn't exist and
/// copies all files and subdirectories from the source to the destination.
///
/// # Arguments
/// * `src` - Source directory path
/// * `dst` - Destination directory path
///
/// # Errors
/// Returns an `std::io::Error` if copying fails
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// Adds all files in a directory to a Git index recursively.
///
/// This function traverses the directory tree and adds all files to the Git index,
/// creating a tree that can be committed. It skips .git directories.
///
/// # Arguments
/// * `index` - Mutable reference to the Git index
/// * `dir` - Directory path to add to the index
///
/// # Errors
/// Returns a `git2::Error` if adding files to the index fails
pub fn add_directory_to_index(index: &mut git2::Index, dir: &Path) -> Result<(), git2::Error> {
    fn add_recursive(
        index: &mut git2::Index,
        dir: &Path,
        prefix: &Path,
    ) -> Result<(), git2::Error> {
        for entry in std::fs::read_dir(dir).map_err(|e| git2::Error::from_str(&e.to_string()))? {
            let entry = entry.map_err(|e| git2::Error::from_str(&e.to_string()))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|e| git2::Error::from_str(&e.to_string()))?;

            // Skip .git directories
            if path.ends_with(".git") {
                continue;
            }

            if file_type.is_file() {
                let relative_path = path.strip_prefix(prefix).unwrap();
                index.add_path(relative_path)?;
            } else if file_type.is_dir() {
                add_recursive(index, &path, prefix)?;
            }
        }
        Ok(())
    }

    add_recursive(index, dir, dir)
}
