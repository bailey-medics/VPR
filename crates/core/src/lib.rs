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

pub mod patient;

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

// Re-export patient types
pub use patient::{FullRecord, PatientService};

#[allow(clippy::single_component_path_imports)]
use serde_yaml;
use std::fs;
use std::path::Path;

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
