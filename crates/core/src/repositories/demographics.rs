//! Patient demographics management.
//!
//! This module provides functionality for initialising and updating patient
//! demographic information within the VPR system. It handles:
//!
//! - Creation of new patient records with unique UUIDs
//! - Storage in a sharded directory structure under `patient_data/demographics/`
//! - Version control using Git with signed commits
//! - Updates to patient name and birth date information
//!
//! ## Storage Layout
//!
//! Demographics are stored as JSON files in a sharded structure:
//!
//! ```text
//! demographics/
//!   <s1>/
//!     <s2>/
//!       <uuid>/
//!         patient.json    # FHIR-like patient resource
//!         .git/           # Git repository for versioning
//! ```
//!
//! where `s1` and `s2` are the first four hex characters of the UUID, providing
//! scalable directory sharding.
//!
//! ## Pure Data Operations
//!
//! This module contains **only** data operations—no API concerns such as
//! authentication, HTTP/gRPC servers, or service interfaces. API-level logic
//! belongs in `api-grpc`, `api-rest`, or `api-shared`.

use crate::author::Author;
use crate::config::CoreConfig;
use crate::constants::{DEMOGRAPHICS_DIR_NAME, PATIENT_JSON_FILENAME};
use crate::error::{PatientError, PatientResult};
use crate::versioned_files::{
    DemographicsDomain::Record, VersionedFileService, VprCommitAction, VprCommitDomain,
    VprCommitMessage,
};
use crate::ShardableUuid;
use api_shared::pb;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

/// Represents a patient demographics record in FHIR-like format.
///
/// Contains basic demographic information for a patient, stored as JSON
/// in `patient.json` files within each patient's sharded directory.
#[derive(Serialize, Deserialize)]
struct Demographics {
    resource_type: String,
    id: String,
    name: Vec<Name>,
    birth_date: String,
    created_at: String,
}

/// Represents a name component of a patient.
///
/// Follows FHIR naming conventions with family and given name fields.
#[derive(Serialize, Deserialize)]
struct Name {
    #[serde(rename = "use")]
    use_: String,
    family: String,
    given: Vec<String>,
}

/// Service for managing patient demographics operations.
///
/// This service handles creation, updates, and listing of patient demographic
/// records. All operations are version-controlled via Git repositories in each
/// patient's directory.
#[derive(Clone)]
pub struct DemographicsService {
    cfg: Arc<CoreConfig>,
}

impl DemographicsService {
    /// Creates a new demographics service.
    ///
    /// # Arguments
    ///
    /// * `cfg` - Core configuration containing patient data directory paths
    pub fn new(cfg: Arc<CoreConfig>) -> Self {
        Self { cfg }
    }
}

impl DemographicsService {
    /// Initialises a new patient demographics record.
    ///
    /// Creates a new patient with a unique UUID, stores the initial demographics
    /// in a JSON file within a sharded directory structure, and initialises a Git
    /// repository for version control.
    ///
    /// # Arguments
    ///
    /// * `author` - Author information for the initial Git commit
    /// * `care_location` - High-level organisational location for the commit (e.g., hospital name)
    ///
    /// # Returns
    ///
    /// The UUID of the newly created patient as a string.
    ///
    /// # Errors
    ///
    /// Returns `PatientError` if:
    /// - JSON serialisation of patient data fails
    /// - Patient directory cannot be created
    /// - `patient.json` file cannot be written
    /// - Git repository initialisation or commit fails
    pub fn initialise(&self, author: Author, care_location: String) -> PatientResult<String> {
        let data_dir = self.cfg.patient_data_dir();

        let demographics_uuid = ShardableUuid::new();
        let created_at = Utc::now().to_rfc3339();

        // Create initial patient JSON
        let patient = Demographics {
            resource_type: "Patient".to_string(),
            id: demographics_uuid.to_string(),
            name: vec![],               // Empty initially
            birth_date: "".to_string(), // Empty initially
            created_at: created_at.clone(),
        };
        let json = serde_json::to_string_pretty(&patient).map_err(PatientError::Serialization)?;

        // Create the demographics directory
        let demographics_dir = data_dir.join(DEMOGRAPHICS_DIR_NAME);
        // Note: demographics directory creation moved to startup validation in main.rs

        let patient_dir = demographics_uuid.sharded_dir(&demographics_dir);
        fs::create_dir_all(&patient_dir).map_err(PatientError::PatientDirCreation)?;

        let filename = patient_dir.join(PATIENT_JSON_FILENAME);
        fs::write(&filename, json).map_err(PatientError::FileWrite)?;

        // Initialise Git repository for the patient
        let repo = VersionedFileService::init(&patient_dir)?;
        let msg = VprCommitMessage::new(
            VprCommitDomain::Demographics(Record),
            VprCommitAction::Create,
            "Demographics record created",
            care_location,
        )?;
        repo.commit_paths(&author, &msg, &[PathBuf::from(PATIENT_JSON_FILENAME)])?;

        Ok(demographics_uuid.to_string())
    }

    /// Updates the demographics of an existing patient.
    ///
    /// Reads the existing patient JSON file, updates the name and birth date fields,
    /// and writes the changes back to the file. This operation does not create a new
    /// Git commit—callers must commit changes separately if needed.
    ///
    /// # Arguments
    ///
    /// * `demographics_uuid` - UUID of the patient to update
    /// * `given_names` - Vector of given names for the patient
    /// * `last_name` - Family/last name of the patient
    /// * `birth_date` - Birth date of the patient as a string
    ///
    /// # Errors
    ///
    /// Returns `PatientError` if:
    /// - UUID cannot be parsed
    /// - `patient.json` file cannot be read, deserialised, serialised, or written
    pub fn update(
        &self,
        demographics_uuid: &str,
        given_names: Vec<String>,
        last_name: &str,
        birth_date: &str,
    ) -> PatientResult<()> {
        let data_dir = self.cfg.patient_data_dir();
        let demographics_dir = data_dir.join(DEMOGRAPHICS_DIR_NAME);

        let demographics_uuid = ShardableUuid::parse(demographics_uuid)?;
        let patient_dir = demographics_uuid.sharded_dir(&demographics_dir);
        let filename = patient_dir.join(PATIENT_JSON_FILENAME);

        // Read existing patient.json
        let existing_json = fs::read_to_string(&filename).map_err(PatientError::FileRead)?;
        let mut patient: Demographics =
            serde_json::from_str(&existing_json).map_err(PatientError::Deserialization)?;

        // Update only the specified fields
        patient.name = vec![Name {
            use_: "official".to_string(),
            family: last_name.to_string(),
            given: given_names,
        }];
        patient.birth_date = birth_date.to_string();

        // Write back the updated JSON
        let json = serde_json::to_string_pretty(&patient).map_err(PatientError::Serialization)?;
        fs::write(&filename, json).map_err(PatientError::FileWrite)?;

        Ok(())
    }

    /// Lists all patient records from the file system.
    ///
    /// Traverses the sharded directory structure under `patient_data/demographics/`
    /// and reads all `patient.json` files to reconstruct patient records.
    ///
    /// # Returns
    ///
    /// Vector of protobuf `Patient` messages containing all found patient records.
    /// Individual patient files that cannot be parsed are logged as warnings and skipped.
    ///
    /// # Directory Structure
    ///
    /// Expects patients stored in:
    /// ```text
    /// <patient_data_dir>/demographics/<s1>/<s2>/<uuid>/patient.json
    /// ```
    /// where `s1`/`s2` are the first four hex characters of the UUID.
    pub fn list_patients(&self) -> Vec<pb::Patient> {
        let data_dir = self.cfg.patient_data_dir();

        let mut patients = Vec::new();

        let demographics_dir = data_dir.join(DEMOGRAPHICS_DIR_NAME);
        let s1_iter = match fs::read_dir(&demographics_dir) {
            Ok(it) => it,
            Err(_) => return patients,
        };
        for s1 in s1_iter.flatten() {
            let s1_path = s1.path();
            if !s1_path.is_dir() {
                continue;
            }

            let s2_iter = match fs::read_dir(&s1_path) {
                Ok(it) => it,
                Err(_) => continue,
            };

            for s2 in s2_iter.flatten() {
                let s2_path = s2.path();
                if !s2_path.is_dir() {
                    continue;
                }

                let id_iter = match fs::read_dir(&s2_path) {
                    Ok(it) => it,
                    Err(_) => continue,
                };

                for id_ent in id_iter.flatten() {
                    let id_path = id_ent.path();
                    if !id_path.is_dir() {
                        continue;
                    }

                    let patient_path = id_path.join(PATIENT_JSON_FILENAME);
                    if !patient_path.is_file() {
                        continue;
                    }

                    if let Ok(contents) = fs::read_to_string(&patient_path) {
                        #[derive(Deserialize)]
                        struct StoredPatient {
                            name: Vec<StoredName>,
                            #[serde(default)]
                            created_at: String,
                        }

                        #[derive(Deserialize)]
                        struct StoredName {
                            family: String,
                            given: Vec<String>,
                        }

                        if let Ok(sp) = serde_json::from_str::<StoredPatient>(&contents) {
                            let id = id_path
                                .file_name()
                                .and_then(|os| os.to_str())
                                .unwrap_or("")
                                .to_string();

                            // Extract name information
                            let (first_name, last_name) = if let Some(name) = sp.name.first() {
                                let first_name = name.given.first().cloned().unwrap_or_default();
                                (first_name, name.family.clone())
                            } else {
                                (String::new(), String::new())
                            };

                            patients.push(pb::Patient {
                                id,
                                first_name,
                                last_name,
                                created_at: sp.created_at,
                                national_id: String::new(), // Not implemented in current demographics
                            });
                        } else {
                            tracing::warn!(
                                "failed to parse patient.json: {}",
                                patient_path.display()
                            );
                        }
                    }
                }
            }
        }

        patients
    }
}
