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

pub mod clinical;

impl PatientService {
    /// Creates a new instance of PatientService.
    ///
    /// # Returns
    /// A new `PatientService` instance ready to handle patient operations.
    pub fn new() -> Self {
        Self
    }

    pub fn initialise_clinical(&self, author: Author) -> PatientResult<String> {
        clinical::initialise_clinical(author)
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
                            #[serde(default)]
                            national_id: Option<String>,
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
                                national_id: sp.national_id.unwrap_or_default(),
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
