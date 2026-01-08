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
pub mod clinical;
pub mod config;
pub mod constants;
pub mod demographics;
pub(crate) mod git;
pub mod repo;
pub(crate) mod uuid;
pub mod validation;

// Use the shared api-shared crate for generated protobuf types.
pub use api_shared::pb;

// Re-export commonly used constants
pub use constants::DEFAULT_PATIENT_DATA_DIR;

pub use config::CoreConfig;

// Re-export author types
pub use author::{
    extract_embedded_commit_signature, Author, AuthorRegistration, EmbeddedCommitSignature,
};

// Re-export repo utilities
pub use repo::{add_directory_to_index, copy_dir_recursive};

#[allow(clippy::single_component_path_imports)]
use serde_yaml;
use std::fs;
use std::path::Path;

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
#[derive(Clone)]
pub struct PatientService {
    cfg: std::sync::Arc<CoreConfig>,
}

impl PatientService {
    /// Creates a new instance of PatientService.
    ///
    /// # Returns
    /// A new `PatientService` instance ready to handle patient operations.
    pub fn new(cfg: std::sync::Arc<CoreConfig>) -> Self {
        Self { cfg }
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
    /// Returns a `PatientError` if:
    /// - demographics initialisation or update fails,
    /// - clinical initialisation fails,
    /// - linking clinical to demographics fails.
    pub fn initialise_full_record(
        &self,
        author: Author,
        care_location: String,
        given_names: Vec<String>,
        last_name: String,
        birth_date: String,
        namespace: Option<String>,
    ) -> PatientResult<FullRecord> {
        let demographics_service = crate::demographics::DemographicsService::new(self.cfg.clone());
        // Initialise demographics
        let demographics_uuid =
            demographics_service.initialise(author.clone(), care_location.clone())?;

        // Update demographics with patient information
        demographics_service.update(&demographics_uuid, given_names, &last_name, &birth_date)?;

        // Initialise clinical
        let clinical_service = crate::clinical::ClinicalService::new(self.cfg.clone());
        let clinical_uuid = clinical_service.initialise(author.clone(), care_location.clone())?;

        // Link clinical to demographics
        clinical_service.link_to_demographics(
            &author,
            care_location,
            &clinical_uuid.simple().to_string(),
            &demographics_uuid,
            namespace,
        )?;

        Ok(FullRecord {
            demographics_uuid,
            clinical_uuid: clinical_uuid.simple().to_string(),
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
