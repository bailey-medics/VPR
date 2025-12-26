//! Patient clinical records management.
//!
//! This module handles the initialization and management of clinical records
//! for patients. It includes creating clinical entries with timestamps,
//! initializing Git repositories for version control, and writing EHR status
//! information in YAML format.

use crate::Author;
use chrono::Utc;
use git2;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::fs;
use std::path::Path;
use uuid::Uuid;

use crate::{PatientError, PatientResult};

/// Represents a clinical record for a patient.
/// Contains metadata such as creation timestamp and placeholders for clinical data.
#[derive(Serialize, Deserialize)]
struct Clinical {
    created_at: String,
    // Add other clinical fields as needed, e.g., notes, etc.
}

/// Represents the EHR status information in openEHR format.
/// This struct models the EHR status archetype for patient records.
#[derive(Serialize, Deserialize)]
struct EhrStatus {
    archetype_node_id: String,
    name: Name,
    is_modifiable: bool,
    is_queryable: bool,
    subject: Subject,
}

/// Represents a name value in the EHR status.
#[derive(Serialize, Deserialize)]
struct Name {
    value: String,
}

/// Represents the subject of the EHR status, linking to the patient.
#[derive(Serialize, Deserialize)]
struct Subject {
    external_ref: ExternalRef,
}

/// Represents an external reference to the patient in the EHR system.
#[derive(Serialize, Deserialize)]
struct ExternalRef {
    namespace: String,
    #[serde(rename = "type")]
    type_: String,
    id: String,
}

/// Service for managing clinical record operations.
#[derive(Default, Clone)]
pub struct ClinicalService;

impl ClinicalService {
    /// Initializes a new clinical record for a patient.
    ///
    /// This function creates a new clinical entry with a unique UUID, stores it
    /// in a JSON file within a sharded directory structure, and initializes a
    /// Git repository for version control.
    ///
    /// # Arguments
    ///
    /// * `author` - The author information for the initial Git commit.
    ///
    /// # Returns
    ///
    /// Returns the UUID of the newly created clinical record as a string.
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if any step in the initialization fails, such as
    /// directory creation, file writing, or Git operations.
    pub fn initialise(&self, author: Author) -> PatientResult<String> {
        // Determine storage directory from environment
        let base = std::env::var("PATIENT_DATA_DIR").unwrap_or_else(|_| "patient_data".into());
        let data_dir = Path::new(&base);
        fs::create_dir_all(data_dir).map_err(PatientError::StorageDirCreation)?;

        // Generate uuid and a 32-hex form without hyphens for directory naming
        let raw_uuid = Uuid::new_v4().to_string();
        let id = raw_uuid.replace('-', "");
        let created_at = Utc::now().to_rfc3339();

        let clinical = Clinical {
            created_at: created_at.clone(),
        };

        // Create the clinical directory
        let clinical_dir = data_dir.join("clinical");
        fs::create_dir_all(&clinical_dir).map_err(PatientError::StorageDirCreation)?;

        // Shard into two-level hex dirs from first 4 chars of the 32-char id within clinical
        let id_lower = id.to_lowercase();
        let s1 = &id_lower[0..2];
        let s2 = &id_lower[2..4];
        let patient_dir = clinical_dir.join(s1).join(s2).join(&id_lower);
        fs::create_dir_all(&patient_dir).map_err(PatientError::PatientDirCreation)?;

        let filename = patient_dir.join("clinical.json");
        let json = serde_json::to_string_pretty(&clinical).map_err(PatientError::Serialization)?;
        fs::write(&filename, json).map_err(PatientError::FileWrite)?;

        // Initialize Git repository for the patient
        let repo = git2::Repository::init(&patient_dir).map_err(PatientError::GitInit)?;

        // Create initial commit with clinical.json
        let mut index = repo.index().map_err(PatientError::GitIndex)?;
        index
            .add_path(std::path::Path::new("clinical.json"))
            .map_err(PatientError::GitAdd)?;

        let tree_id = index.write_tree().map_err(PatientError::GitWriteTree)?;
        let tree = repo.find_tree(tree_id).map_err(PatientError::GitFindTree)?;

        let sig = git2::Signature::now(&author.name, &author.email)
            .map_err(PatientError::GitSignature)?;
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "Initial clinical record",
            &tree,
            &[],
        )
        .map_err(PatientError::GitCommit)?;

        Ok(id)
    }

    /// Links the clinical record to the patient's demographics.
    ///
    /// This function creates an EHR status YAML file linking the clinical record
    /// to the patient's demographics via external references.
    ///
    /// # Arguments
    ///
    /// * `clinical_uuid` - The UUID of the clinical record.
    /// * `demographics_uuid` - The UUID of the associated patient demographics.
    /// * `namespace` - Optional namespace for the external reference; defaults to
    ///   the VPR_NAMESPACE environment variable or "vpr.dev.1".
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if serialization or file writing fails.
    pub fn link_to_demographics(
        &self,
        clinical_uuid: &str,
        demographics_uuid: &str,
        namespace: Option<String>,
    ) -> PatientResult<()> {
        let namespace = namespace.unwrap_or_else(|| {
            std::env::var("VPR_NAMESPACE").unwrap_or_else(|_| "vpr.dev.1".into())
        });

        let base = std::env::var("PATIENT_DATA_DIR").unwrap_or_else(|_| "patient_data".into());
        let data_dir = Path::new(&base);
        let clinical_dir = data_dir.join("clinical");

        let id_lower = clinical_uuid.to_lowercase();
        let s1 = &id_lower[0..2];
        let s2 = &id_lower[2..4];
        let patient_dir = clinical_dir.join(s1).join(s2).join(&id_lower);

        let ehr_status = EhrStatus {
            archetype_node_id: "openEHR-EHR-STATUS.ehr_status.v1".to_string(),
            name: Name {
                value: "EHR Status".to_string(),
            },
            is_modifiable: true,
            is_queryable: true,
            subject: Subject {
                external_ref: ExternalRef {
                    namespace: format!("vpr://{}/mpi", namespace),
                    type_: "PERSON".to_string(),
                    id: demographics_uuid.to_string(),
                },
            },
        };

        let yaml = serde_yaml::to_string(&ehr_status).map_err(PatientError::YamlSerialization)?;
        let filename = patient_dir.join("ehr_status.yaml");
        fs::write(&filename, yaml).map_err(PatientError::FileWrite)?;

        Ok(())
    }
}
