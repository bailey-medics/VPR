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

// Use the shared api-shared crate for generated protobuf types.
pub use api_shared::pb;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use uuid::Uuid;

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
    #[error("failed to serialize patient: {0}")]
    Serialization(serde_json::Error),
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

    /// Creates a new patient record with the given name information.
    ///
    /// This method performs the following operations:
    /// 1. Validates that both first and last names are non-empty
    /// 2. Generates a unique UUID for the patient
    /// 3. Creates a sharded directory structure under `PATIENT_DATA_DIR`
    /// 4. Stores patient demographics as JSON in `demographics.json`
    /// 5. Initializes a Git repository and creates an initial commit
    ///
    /// # Arguments
    /// * `first_name` - The patient's first name (will be trimmed)
    /// * `last_name` - The patient's last name (will be trimmed)
    /// * `author_name` - Name of the person creating the record (required)
    /// * `author_email` - Email of the person creating the record (required)
    ///
    /// # Returns
    /// * `Ok(CreatePatientRes)` - Contains the filename and patient data on success
    /// * `Err(PatientError)` - If validation fails or file/git operations error
    ///
    /// # Storage Format
    /// Patients are stored in a sharded directory structure with Git versioning:
    /// `<PATIENT_DATA_DIR>/<s1>/<s2>/<32hex-uuid>/.git/` and `demographics.json`
    /// where s1/s2 are the first 4 hex characters of the UUID.
    pub fn create_patient(
        &self,
        first_name: String,
        last_name: String,
        author_name: String,
        author_email: String,
    ) -> PatientResult<pb::CreatePatientRes> {
        let first = first_name.trim();
        let last = last_name.trim();
        if first.is_empty() || last.is_empty() {
            return Err(PatientError::InvalidInput);
        }

        // Determine storage directory from environment (matches compose.dev.yml)
        // Store each patient under <PATIENT_DATA_DIR>/<2hex>/<2hex>/<32hex-uuid>/demographics.json
        let base = std::env::var("PATIENT_DATA_DIR").unwrap_or_else(|_| "/patient_data".into());
        let data_dir = Path::new(&base);
        fs::create_dir_all(data_dir).map_err(PatientError::StorageDirCreation)?;

        // generate uuid and a 32-hex form without hyphens for directory naming
        let raw_uuid = Uuid::new_v4().to_string();
        let id = raw_uuid.replace('-', "");
        let created_at = Utc::now().to_rfc3339();

        #[derive(Serialize, Deserialize)]
        struct StoredPatient {
            first_name: String,
            last_name: String,
            created_at: String,
        }

        let patient = StoredPatient {
            first_name: first.to_string(),
            last_name: last.to_string(),
            created_at: created_at.clone(),
        };

        // shard into two-level hex dirs from first 4 chars of the 32-char id
        let id_lower = id.to_lowercase();
        let s1 = &id_lower[0..2];
        let s2 = &id_lower[2..4];
        let patient_dir = data_dir.join(s1).join(s2).join(&id_lower);
        fs::create_dir_all(&patient_dir).map_err(PatientError::PatientDirCreation)?;

        let filename = patient_dir.join("demographics.json");
        let json = serde_json::to_string_pretty(&patient).map_err(PatientError::Serialization)?;
        fs::write(&filename, json).map_err(PatientError::FileWrite)?;

        // Initialize Git repository for the patient
        let repo = git2::Repository::init(&patient_dir).map_err(PatientError::GitInit)?;

        // Create initial commit with demographics.json
        let mut index = repo.index().map_err(PatientError::GitIndex)?;
        index
            .add_path(std::path::Path::new("demographics.json"))
            .map_err(PatientError::GitAdd)?;

        let tree_id = index.write_tree().map_err(PatientError::GitWriteTree)?;
        let tree = repo.find_tree(tree_id).map_err(PatientError::GitFindTree)?;

        let sig = git2::Signature::now(&author_name, &author_email)
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

        let resp = pb::CreatePatientRes {
            filename: filename.display().to_string(),
            patient: Some(pb::Patient {
                id,
                first_name: first.to_string(),
                last_name: last.to_string(),
                created_at,
            }),
        };

        Ok(resp)
    }

    /// Lists all patient records from the file system.
    ///
    /// This method traverses the sharded directory structure under `PATIENT_DATA_DIR`
    /// and reads all `demographics.json` files to reconstruct patient records.
    ///
    /// # Returns
    /// A `Vec<pb::Patient>` containing all found patient records. If any individual
    /// patient file cannot be parsed, it will be logged as a warning and skipped.
    ///
    /// # Directory Structure
    /// Expects patients stored in: `<PATIENT_DATA_DIR>/<s1>/<s2>/<32hex-uuid>/demographics.json`
    /// where s1/s2 are the first 4 hex characters of the UUID.
    pub fn list_patients(&self) -> Vec<pb::Patient> {
        let base = std::env::var("PATIENT_DATA_DIR").unwrap_or_else(|_| "/patient_data".into());
        let data_dir = Path::new(&base);

        let mut patients = Vec::new();

        let s1_iter = match fs::read_dir(data_dir) {
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

                    let demo_path = id_path.join("demographics.json");
                    if !demo_path.is_file() {
                        continue;
                    }

                    if let Ok(contents) = fs::read_to_string(&demo_path) {
                        #[derive(serde::Deserialize)]
                        struct StoredPatient {
                            first_name: String,
                            last_name: String,
                            created_at: String,
                        }

                        if let Ok(sp) = serde_json::from_str::<StoredPatient>(&contents) {
                            let id = id_path
                                .file_name()
                                .and_then(|os| os.to_str())
                                .unwrap_or("")
                                .to_string();

                            patients.push(pb::Patient {
                                id,
                                first_name: sp.first_name,
                                last_name: sp.last_name,
                                created_at: sp.created_at,
                            });
                        } else {
                            tracing::warn!("failed to parse demographics: {}", demo_path.display());
                        }
                    }
                }
            }
        }

        patients
    }
}
