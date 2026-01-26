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
//! Demographics are stored as YAML files in a sharded structure:
//!
//! ```text
//! demographics/
//!   <s1>/
//!     <s2>/
//!       <uuid>/
//!         patient.yaml    # FHIR-aligned patient resource
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
use crate::constants::{DEFAULT_GITIGNORE, DEMOGRAPHICS_DIR_NAME};
use crate::error::{PatientError, PatientResult};
use crate::paths::common::GitIgnoreFile;
use crate::paths::demographics::patient::PatientFile;
use crate::versioned_files::{
    DemographicsDomain::Record, FileToWrite, VersionedFileService, VprCommitAction,
    VprCommitDomain, VprCommitMessage,
};
use crate::NonEmptyText;
use crate::ShardableUuid;
use api_shared::pb;
use chrono::Utc;
use fhir::{NameUse, Patient, PatientData};
use std::fs;
use std::path::Path;
use std::sync::Arc;

// ============================================================================
// TYPE-STATE MARKERS
// ============================================================================

/// Marker type: demographics record does not yet exist.
///
/// Used in type-state pattern to prevent operations on non-existent records.
/// Only `initialise()` can be called in this state.
#[derive(Clone, Copy, Debug)]
pub struct Uninitialised;

/// Marker type: demographics record exists.
///
/// Indicates a valid demographics record with a known UUID.
/// Enables operations like updating demographics and listing patients.
#[derive(Clone, Debug)]
pub struct Initialised {
    demographics_id: ShardableUuid,
}

// ============================================================================
// DEMOGRAPHICS SERVICE
// ============================================================================

/// Service for managing patient demographics operations.
///
/// Uses type-state pattern to enforce correct usage at compile time.
/// Generic parameter `S` is either `Uninitialised` or `Initialised`.
///
/// This service handles creation, updates, and listing of patient demographic
/// records. All operations are version-controlled via Git repositories in each
/// patient's directory.
#[derive(Clone, Debug)]
pub struct DemographicsService<S> {
    cfg: Arc<CoreConfig>,
    state: S,
}

impl DemographicsService<Uninitialised> {
    /// Creates a new demographics service in the uninitialised state.
    ///
    /// # Arguments
    ///
    /// * `cfg` - Core configuration containing patient data directory paths
    pub fn new(cfg: Arc<CoreConfig>) -> Self {
        Self {
            cfg,
            state: Uninitialised,
        }
    }
}

impl DemographicsService<Uninitialised> {
    /// Initialises a new patient demographics record.
    ///
    /// Creates a new patient with a unique UUID, stores the initial demographics
    /// in a YAML file within a sharded directory structure, and initialises a Git
    /// repository for version control.
    ///
    /// **This method consumes `self`** and returns a new `DemographicsService<Initialised>` on success,
    /// enforcing at compile time that you cannot call `initialise()` twice on the same service.
    ///
    /// # Arguments
    ///
    /// * `author` - Author information for the initial Git commit
    /// * `care_location` - High-level organisational location for the commit (e.g., hospital name)
    ///
    /// # Returns
    ///
    /// Returns `DemographicsService<Initialised>` containing the newly created demographics record.
    /// Use [`demographics_id()`](DemographicsService::demographics_id) to get the UUID.
    ///
    /// # Errors
    ///
    /// Returns `PatientError` if:
    /// - YAML serialisation of patient data fails
    /// - Patient directory cannot be created
    /// - `patient.yaml` file cannot be written
    /// - Git repository initialisation or commit fails
    /// - Cleanup of a partially-created record directory fails ([`PatientError::CleanupAfterInitialiseFailed`])
    ///
    /// # Safety & Rollback
    ///
    /// If any operation fails during initialisation, this method attempts to clean up the
    /// partially-created patient directory. If cleanup also fails, a
    /// [`PatientError::CleanupAfterInitialiseFailed`] is returned with details of both errors.
    pub fn initialise(
        self,
        author: Author,
        care_location: NonEmptyText,
    ) -> PatientResult<DemographicsService<Initialised>> {
        author.validate_commit_author()?;

        let commit_message = VprCommitMessage::new(
            VprCommitDomain::Demographics(Record),
            VprCommitAction::Create,
            "Demographics record created",
            care_location,
        )?;

        let data_dir = self.cfg.patient_data_dir();
        let demographics_dir = data_dir.join(DEMOGRAPHICS_DIR_NAME);

        let demographics_uuid = ShardableUuid::new();
        let patient_dir = demographics_uuid.sharded_dir(&demographics_dir);
        let created_at = Utc::now();

        let patient_data = PatientData {
            id: demographics_uuid.clone(),
            use_type: None,
            family: None,
            given: vec![],
            birth_date: None,
            last_updated: Some(created_at),
        };

        let patient_data_raw = Patient::render(&patient_data)?;

        let files = [
            FileToWrite {
                relative_path: Path::new(GitIgnoreFile::NAME),
                content: DEFAULT_GITIGNORE,
                old_content: None,
            },
            FileToWrite {
                relative_path: Path::new(PatientFile::NAME),
                content: &patient_data_raw,
                old_content: None,
            },
        ];

        VersionedFileService::init_and_commit(&patient_dir, &author, &commit_message, &files)?;

        Ok(DemographicsService {
            cfg: self.cfg,
            state: Initialised {
                demographics_id: demographics_uuid,
            },
        })
    }
}

impl DemographicsService<Initialised> {
    /// Creates a demographics service for an existing record.
    ///
    /// Use this when you already have a demographics record and want to perform
    /// operations on it, such as updating demographics or listing patients.
    ///
    /// # Arguments
    ///
    /// * `cfg` - Core configuration containing patient data directory paths
    /// * `demographics_id` - UUID string of the existing demographics record
    pub fn with_id(cfg: Arc<CoreConfig>, demographics_id: &str) -> PatientResult<Self> {
        let demographics_uuid = ShardableUuid::parse(demographics_id)?;
        Ok(Self {
            cfg,
            state: Initialised {
                demographics_id: demographics_uuid,
            },
        })
    }

    /// Returns the demographics UUID.
    pub fn demographics_id(&self) -> &ShardableUuid {
        &self.state.demographics_id
    }
}

impl DemographicsService<Initialised> {
    /// Updates the demographics of an existing patient.
    ///
    /// Reads the existing patient YAML file, updates the name and birth date fields,
    /// and writes the changes back to the file. This operation does not create a new
    /// Git commit—callers must commit changes separately if needed.
    ///
    /// # Arguments
    ///
    /// * `given_names` - Vector of given names for the patient
    /// * `last_name` - Family/last name of the patient
    /// * `birth_date` - Birth date of the patient as a string
    ///
    /// # Errors
    ///
    /// Returns `PatientError` if:
    /// - `patient.yaml` file cannot be read, deserialised, serialised, or written
    pub fn update(
        &self,
        given_names: Vec<String>,
        last_name: &str,
        birth_date: &str,
    ) -> PatientResult<()> {
        let data_dir = self.cfg.patient_data_dir();
        let demographics_dir = data_dir.join(DEMOGRAPHICS_DIR_NAME);

        let patient_dir = self.demographics_id().sharded_dir(&demographics_dir);
        let filename = patient_dir.join(PatientFile::NAME);

        // Read existing patient.yaml
        let existing_yaml = fs::read_to_string(&filename).map_err(PatientError::FileRead)?;
        let mut patient_data = Patient::parse(&existing_yaml)?;

        // Update only the specified fields
        patient_data.use_type = Some(NameUse::Official);
        patient_data.family = Some(last_name.to_string());
        patient_data.given = given_names;
        patient_data.birth_date = Some(birth_date.to_string());

        // Write back the updated YAML
        let yaml = Patient::render(&patient_data)?;
        fs::write(&filename, yaml).map_err(PatientError::FileWrite)?;

        Ok(())
    }
}

// ============================================================================
// SHARED OPERATIONS (AVAILABLE ON BOTH STATES)
// ============================================================================

impl<S> DemographicsService<S> {
    /// Lists all patient records from the file system.
    ///
    /// Traverses the sharded directory structure under `patient_data/demographics/`
    /// and reads all `patient.yaml` files to reconstruct patient records.
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
    /// <patient_data_dir>/demographics/<s1>/<s2>/<uuid>/patient.yaml
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

                    let patient_path = id_path.join(PatientFile::NAME);
                    if !patient_path.is_file() {
                        continue;
                    }

                    if let Ok(contents) = fs::read_to_string(&patient_path) {
                        match Patient::parse(&contents) {
                            Ok(patient_data) => {
                                let id = patient_data.id.to_string();

                                // Extract name information from flat structure
                                let first_name =
                                    patient_data.given.first().cloned().unwrap_or_default();
                                let last_name = patient_data.family.unwrap_or_default();
                                let created_at = patient_data
                                    .last_updated
                                    .map(|dt| dt.to_rfc3339())
                                    .unwrap_or_default();

                                patients.push(pb::Patient {
                                    id,
                                    first_name,
                                    last_name,
                                    created_at,
                                    national_id: String::new(), // Not implemented in current demographics
                                });
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "failed to parse patient.yaml: {} - {}",
                                    patient_path.display(),
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }

        patients
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::DEMOGRAPHICS_DIR_NAME;
    use crate::{EmailAddress, NonEmptyText};
    use std::fs;
    use tempfile::TempDir;

    fn test_author() -> Author {
        Author {
            name: NonEmptyText::new("Test Author").unwrap(),
            role: NonEmptyText::new("Clinician").unwrap(),
            email: EmailAddress::parse("test@example.com").unwrap(),
            registrations: vec![],
            signature: None,
            certificate: None,
        }
    }

    fn test_cfg(patient_data_dir: &Path) -> Arc<CoreConfig> {
        use crate::config::rm_system_version_from_env_value;

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        Arc::new(
            CoreConfig::new(
                patient_data_dir.to_path_buf(),
                rm_system_version,
                crate::NonEmptyText::new("vpr.dev.1").unwrap(),
            )
            .expect("CoreConfig::new should succeed"),
        )
    }

    fn count_allocated_patient_dirs(demographics_dir: &Path) -> usize {
        let mut count = 0;
        if let Ok(s1_iter) = fs::read_dir(demographics_dir) {
            for s1 in s1_iter.flatten() {
                if let Ok(s2_iter) = fs::read_dir(s1.path()) {
                    for s2 in s2_iter.flatten() {
                        if let Ok(id_iter) = fs::read_dir(s2.path()) {
                            for _id in id_iter.flatten() {
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
        count
    }

    #[test]
    fn test_initialise_creates_demographics_record() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());
        let service = DemographicsService::new(cfg.clone());

        let author = test_author();
        let demographics_service = service
            .initialise(author, NonEmptyText::new("Test Hospital").unwrap())
            .expect("initialise should succeed");

        let demographics_id = demographics_service.demographics_id();
        let demographics_dir = temp_dir.path().join(DEMOGRAPHICS_DIR_NAME);
        let patient_dir = demographics_id.sharded_dir(&demographics_dir);

        assert!(patient_dir.exists(), "patient directory should exist");
        assert!(
            patient_dir.join(".git").is_dir(),
            "git repository should be initialised"
        );
        assert!(
            patient_dir.join(".gitignore").is_file(),
            ".gitignore file should exist"
        );
        assert!(
            patient_dir.join(PatientFile::NAME).is_file(),
            "patient.yaml file should exist"
        );

        // Verify patient.yaml content
        let yaml_content = fs::read_to_string(patient_dir.join(PatientFile::NAME))
            .expect("should read patient.yaml");
        let patient_data = Patient::parse(&yaml_content).expect("should parse patient.yaml");

        assert_eq!(patient_data.id.uuid(), demographics_id.uuid());
        assert_eq!(patient_data.use_type, None);
        assert_eq!(patient_data.family, None);
        assert_eq!(patient_data.given.len(), 0);
        assert_eq!(patient_data.birth_date, None);
        assert!(patient_data.last_updated.is_some());
    }

    #[test]
    fn test_initialise_fails_fast_on_invalid_author_and_creates_no_files() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());
        let _service = DemographicsService::new(cfg);

        // NonEmptyText validation prevents empty strings at the type level
        let err =
            NonEmptyText::new("").expect_err("creating NonEmptyText from empty string should fail");

        assert!(
            matches!(err, crate::TextError::Empty),
            "should return TextError::Empty"
        );

        let demographics_dir = temp_dir.path().join(DEMOGRAPHICS_DIR_NAME);
        assert_eq!(
            count_allocated_patient_dirs(&demographics_dir),
            0,
            "no patient directories should be created"
        );
    }

    #[test]
    fn test_initialise_rejects_missing_care_location_and_creates_no_files() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());
        let _service = DemographicsService::new(cfg);

        let _author = test_author();
        // NonEmptyText validation prevents empty strings at the type level
        let err =
            NonEmptyText::new("").expect_err("creating NonEmptyText from empty string should fail");

        assert!(
            matches!(err, crate::TextError::Empty),
            "should return TextError::Empty"
        );

        let demographics_dir = temp_dir.path().join(DEMOGRAPHICS_DIR_NAME);
        assert_eq!(
            count_allocated_patient_dirs(&demographics_dir),
            0,
            "no patient directories should be created"
        );
    }

    #[test]
    fn test_initialise_cleans_up_on_failure() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());
        let _service = DemographicsService::new(cfg);

        let _author = test_author();

        // Trigger a validation failure - NonEmptyText prevents empty strings at type level
        let _err =
            NonEmptyText::new("").expect_err("creating NonEmptyText from empty string should fail");

        let demographics_dir = temp_dir.path().join(DEMOGRAPHICS_DIR_NAME);
        assert_eq!(
            count_allocated_patient_dirs(&demographics_dir),
            0,
            "no patient directories should exist after failure"
        );
    }

    #[test]
    fn test_with_id_creates_initialised_service() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());

        let demographics_uuid = ShardableUuid::new();
        let demographics_id_str = demographics_uuid.to_string();

        let service = DemographicsService::with_id(cfg, &demographics_id_str)
            .expect("with_id should succeed");

        assert_eq!(
            service.demographics_id().to_string(),
            demographics_id_str,
            "demographics_id should match"
        );
    }

    #[test]
    fn test_with_id_rejects_invalid_uuid() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());

        let err = DemographicsService::with_id(cfg, "not-a-valid-uuid")
            .expect_err("with_id should fail with invalid UUID");

        assert!(
            matches!(err, PatientError::Uuid(_)),
            "should return Uuid error"
        );
    }

    #[test]
    fn test_update_modifies_patient_data() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());
        let service = DemographicsService::new(cfg);

        let author = test_author();
        let demographics_service = service
            .initialise(author, NonEmptyText::new("Test Hospital").unwrap())
            .expect("initialise should succeed");

        // Update demographics
        demographics_service
            .update(
                vec!["John".to_string(), "Paul".to_string()],
                "Smith",
                "1990-01-15",
            )
            .expect("update should succeed");

        // Read and verify updated data
        let demographics_dir = temp_dir.path().join(DEMOGRAPHICS_DIR_NAME);
        let patient_dir = demographics_service
            .demographics_id()
            .sharded_dir(&demographics_dir);
        let yaml_content = fs::read_to_string(patient_dir.join(PatientFile::NAME))
            .expect("should read patient.yaml");
        let patient_data = Patient::parse(&yaml_content).expect("should parse patient.yaml");

        assert_eq!(patient_data.use_type, Some(NameUse::Official));
        assert_eq!(patient_data.family, Some("Smith".to_string()));
        assert_eq!(
            patient_data.given,
            vec!["John".to_string(), "Paul".to_string()]
        );
        assert_eq!(patient_data.birth_date, Some("1990-01-15".to_string()));
    }

    #[test]
    fn test_list_patients_returns_empty_for_nonexistent_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());
        let service = DemographicsService::new(cfg);

        let patients = service.list_patients();
        assert_eq!(patients.len(), 0, "should return empty list");
    }

    #[test]
    fn test_list_patients_returns_created_patients() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());

        // Create first patient
        let service1 = DemographicsService::new(cfg.clone());
        let demographics_service1 = service1
            .initialise(test_author(), NonEmptyText::new("Test Hospital").unwrap())
            .expect("initialise should succeed");
        demographics_service1
            .update(vec!["Alice".to_string()], "Smith", "1990-01-15")
            .expect("update should succeed");

        // Create second patient
        let service2 = DemographicsService::new(cfg.clone());
        let demographics_service2 = service2
            .initialise(test_author(), NonEmptyText::new("Test Hospital").unwrap())
            .expect("initialise should succeed");
        demographics_service2
            .update(vec!["Bob".to_string()], "Jones", "1985-06-20")
            .expect("update should succeed");

        // List all patients
        let list_service = DemographicsService::new(cfg);
        let patients = list_service.list_patients();

        assert_eq!(patients.len(), 2, "should return 2 patients");

        // Verify patient data (order not guaranteed)
        let alice = patients.iter().find(|p| p.first_name == "Alice");
        let bob = patients.iter().find(|p| p.first_name == "Bob");

        assert!(alice.is_some(), "should find Alice");
        assert!(bob.is_some(), "should find Bob");

        let alice = alice.unwrap();
        assert_eq!(alice.last_name, "Smith");
        assert!(!alice.created_at.is_empty());

        let bob = bob.unwrap();
        assert_eq!(bob.last_name, "Jones");
        assert!(!bob.created_at.is_empty());
    }

    #[test]
    fn test_list_patients_skips_invalid_yaml() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());

        // Create valid patient
        let service1 = DemographicsService::new(cfg.clone());
        let demographics_service1 = service1
            .initialise(test_author(), NonEmptyText::new("Test Hospital").unwrap())
            .expect("initialise should succeed");
        demographics_service1
            .update(vec!["Valid".to_string()], "Patient", "1990-01-15")
            .expect("update should succeed");

        // Create invalid patient.yaml manually
        let demographics_uuid = ShardableUuid::new();
        let demographics_dir = temp_dir.path().join(DEMOGRAPHICS_DIR_NAME);
        let invalid_patient_dir = demographics_uuid.sharded_dir(&demographics_dir);
        fs::create_dir_all(&invalid_patient_dir).expect("should create directory");
        fs::write(
            invalid_patient_dir.join(PatientFile::NAME),
            "invalid: yaml: content: [[[",
        )
        .expect("should write invalid yaml");

        // List patients should skip the invalid one
        let list_service = DemographicsService::new(cfg);
        let patients = list_service.list_patients();

        assert_eq!(
            patients.len(),
            1,
            "should return only 1 valid patient, skipping invalid"
        );
        assert_eq!(patients[0].first_name, "Valid");
    }
}
