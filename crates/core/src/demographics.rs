//! Patient demographics management.
//!
//! This module provides functionality for initialising and updating patient
//! demographic information. It handles the creation of new patient records
//! with unique identifiers, storage in a sharded directory structure, and
//! version control using Git. Demographic updates include name and birth date
//! modifications.

use crate::Author;
use chrono;
use git2;
use serde::{Deserialize, Serialize};
use serde_json;
use std::fs;
use tracing;

use crate::{PatientError, PatientResult};

/// Represents a patient record in FHIR-like format.
/// This struct contains basic demographic information for a patient.
#[derive(Serialize, Deserialize)]
struct Patient {
    resource_type: String,
    id: String,
    name: Vec<Name>,
    birth_date: String,
    created_at: String,
}

/// Represents a name component of a patient.
/// Contains the use type, family name, and given names.
#[derive(Serialize, Deserialize)]
struct Name {
    #[serde(rename = "use")]
    use_: String,
    family: String,
    given: Vec<String>,
}

/// Service for managing patient demographics operations.
#[derive(Default, Clone)]
pub struct DemographicsService;

impl DemographicsService {
    /// Initialises a new patient demographics record.
    ///
    /// This function creates a new patient with a unique UUID, stores the initial
    /// demographics in a JSON file within a sharded directory structure, and
    /// initialises a Git repository for version control.
    ///
    /// # Arguments
    ///
    /// * `author` - The author information for the initial Git commit.
    ///
    /// # Returns
    ///
    /// Returns the UUID of the newly created patient as a string.
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if any step in the initialisation fails, such as
    /// directory creation, file writing, or Git operations.
    pub fn initialise(&self, author: Author) -> PatientResult<String> {
        // Determine storage directory from environment
        let data_dir = crate::patient_data_path();

        let demographics_uuid = crate::uuid::UuidService::new();
        let created_at = chrono::Utc::now().to_rfc3339();

        // Create initial patient JSON
        let patient = Patient {
            resource_type: "Patient".to_string(),
            id: demographics_uuid.to_string(),
            name: vec![],               // Empty initially
            birth_date: "".to_string(), // Empty initially
            created_at: created_at.clone(),
        };
        let json = serde_json::to_string_pretty(&patient).map_err(PatientError::Serialization)?;

        // Create the demographics directory
        let demographics_dir = data_dir.join(crate::constants::DEMOGRAPHICS_DIR_NAME);
        // Note: demographics directory creation moved to startup validation in main.rs

        let patient_dir = demographics_uuid.sharded_dir(&demographics_dir);
        fs::create_dir_all(&patient_dir).map_err(PatientError::PatientDirCreation)?;

        let filename = patient_dir.join(crate::constants::PATIENT_JSON_FILENAME);
        fs::write(&filename, json).map_err(PatientError::FileWrite)?;

        // Initialise Git repository for the patient
        let repo = crate::git::GitService::init(&patient_dir)?.into_repo();

        // Create initial commit with demographics.json
        let mut index = repo.index().map_err(PatientError::GitIndex)?;
        index
            .add_path(std::path::Path::new(
                crate::constants::PATIENT_JSON_FILENAME,
            ))
            .map_err(PatientError::GitAdd)?;

        let tree_id = index.write_tree().map_err(PatientError::GitWriteTree)?;
        let tree = repo.find_tree(tree_id).map_err(PatientError::GitFindTree)?;

        let sig = git2::Signature::now(&author.name, &author.email)
            .map_err(PatientError::GitSignature)?;
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "Initial patient record",
            &tree,
            &[],
        )
        .map_err(PatientError::GitCommit)?;

        Ok(demographics_uuid.into_string())
    }

    /// Updates the demographics of an existing patient.
    ///
    /// This function reads the existing patient JSON file, updates the name and
    /// birth date fields, and writes the changes back to the file.
    ///
    /// # Arguments
    ///
    /// * `demographics_uuid` - The UUID of the patient to update.
    /// * `given_names` - A vector of given names for the patient.
    /// * `last_name` - The family/last name of the patient.
    /// * `birth_date` - The birth date of the patient as a string.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if reading, deserializing, serializing, or writing
    /// the file fails.
    pub fn update(
        &self,
        demographics_uuid: &str,
        given_names: Vec<String>,
        last_name: &str,
        birth_date: &str,
    ) -> PatientResult<()> {
        let data_dir = crate::patient_data_path();
        let demographics_dir = data_dir.join(crate::constants::DEMOGRAPHICS_DIR_NAME);

        let demographics_uuid = crate::uuid::UuidService::parse(demographics_uuid)?;
        let patient_dir = demographics_uuid.sharded_dir(&demographics_dir);
        let filename = patient_dir.join(crate::constants::PATIENT_JSON_FILENAME);

        // Read existing patient.json
        let existing_json = fs::read_to_string(&filename).map_err(PatientError::FileRead)?;
        let mut patient: Patient =
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
    /// This method traverses the sharded directory structure under `PATIENT_DATA_DIR`
    /// and reads all `patient.json` files to reconstruct patient records.
    ///
    /// # Returns
    /// A `Vec<pb::Patient>` containing all found patient records. If any individual
    /// patient file cannot be parsed, it will be logged as a warning and skipped.
    ///
    /// # Directory Structure
    /// Expects patients stored in: `<PATIENT_DATA_DIR>/demographics/<s1>/<s2>/<32hex-uuid>/patient.json`
    /// where s1/s2 are the first 4 hex characters of the UUID.
    pub fn list_patients(&self) -> Vec<crate::pb::Patient> {
        let data_dir = crate::patient_data_path();

        let mut patients = Vec::new();

        let demographics_dir = data_dir.join(crate::constants::DEMOGRAPHICS_DIR_NAME);
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

                    let patient_path = id_path.join(crate::constants::PATIENT_JSON_FILENAME);
                    if !patient_path.is_file() {
                        continue;
                    }

                    if let Ok(contents) = fs::read_to_string(&patient_path) {
                        #[derive(serde::Deserialize)]
                        struct StoredPatient {
                            name: Vec<StoredName>,
                            #[serde(default)]
                            created_at: String,
                        }

                        #[derive(serde::Deserialize)]
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

                            patients.push(crate::pb::Patient {
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
