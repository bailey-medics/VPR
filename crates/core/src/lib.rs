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

pub mod error;

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

// Re-export error types
pub use error::{PatientError, PatientResult};

#[allow(clippy::single_component_path_imports)]
use serde_yaml;
use std::fs;
use std::path::Path;

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
