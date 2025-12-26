//! # VPR Core
//!
//! Core business logic for the VPR patient record system.
//!
//! This crate contains pure data operations and file/folder management:
//! - Patient creation and listing with sharded JSON storage
//! - File system operations under `PATIENT_DATA_DIR`
//! - Git-like versioning (future)
//!
//! **No API concerns**: Authentication, HTTP/gRPC servers, or service interfaces belong in `api-grpc`, `api-rest`, or `api-shared`.

pub mod demographics;

// Use the shared api-shared crate for generated protobuf types.
pub use api_shared::pb;

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
    #[error("failed to initialize git repository: {0}")]
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

pub mod clinical;

impl PatientService {
    /// Creates a new instance of PatientService.
    ///
    /// # Returns
    /// A new `PatientService` instance ready to handle patient operations.
    pub fn new() -> Self {
        Self
    }

    /// Initializes a complete patient record with demographics and clinical components.
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
    /// Returns a `PatientError` if any step in the initialization fails.
    pub fn initialise_full_record(
        &self,
        author: Author,
        given_names: Vec<String>,
        last_name: String,
        birth_date: String,
        namespace: Option<String>,
    ) -> PatientResult<FullRecord> {
        let demographics_service = crate::demographics::DemographicsService;
        // Initialize demographics
        let demographics_uuid = demographics_service.initialise(author.clone())?;

        // Update demographics with patient information
        demographics_service.update(&demographics_uuid, given_names, &last_name, &birth_date)?;

        // Initialize clinical
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
