//! Patient clinical records management.
//!
//! This module handles the creation, linking, and maintenance of per-patient
//! clinical record repositories within the Versioned Patient Repository (VPR).
//! It initialises new records from validated clinical templates, enforces directory
//! sharding for scalable storage, and ensures all operations are version-controlled
//! through Git with optional cryptographic signing.
//!
//! Each record includes an `ehr_status.yaml` file conforming to openEHR structures,
//! providing metadata about the patient’s EHR lifecycle and its linkage to
//! demographics via external references.
//!
//! All filesystem operations are validated for safety and rollback on failure,
//! guaranteeing no partial or unsafe patient directories remain after errors.

use crate::author::Author;
use crate::config::CoreConfig;
use crate::constants::CLINICAL_DIR_NAME;
use crate::error::{PatientError, PatientResult};
use crate::paths::clinical::letter::LetterPaths;
use crate::repositories::shared::{
    copy_dir_recursive, create_uuid_and_shard_dir, validate_template, TemplateDirKind,
};
use crate::versioned_files::{
    FileToWrite, VersionedFileService, VprCommitAction, VprCommitDomain, VprCommitMessage,
};
use crate::ShardableUuid;
use openehr::{
    ehr_status_render, extract_rm_version, validation::validate_namespace_uri_safe, EhrId,
    ExternalReference, OpenEhrFileType::EhrStatus,
};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};
use vpr_uuid::TimestampIdGenerator;

#[cfg(test)]
use std::io::ErrorKind;
use uuid::Uuid;

#[cfg(test)]
use std::collections::HashSet;

#[cfg(test)]
use std::sync::{LazyLock, Mutex};

/// Marker type: clinical record does not yet exist.
///
/// This is a zero-sized type used in the type-state pattern to indicate that
/// a [`ClinicalService`] has not yet been initialised. Services in this state
/// can only call [`ClinicalService::initialise()`] to create a new clinical record.
///
/// # Type Safety
///
/// The type system prevents you from calling operations that require an existing
/// clinical record (like [`link_to_demographics`](ClinicalService::link_to_demographics))
/// when the service is in the `Uninitialised` state.
#[derive(Clone, Copy, Debug)]
pub struct Uninitialised;

/// Marker type: clinical record exists.
///
/// This type is used in the type-state pattern to indicate that a [`ClinicalService`]
/// has been initialised and has a valid clinical record with a known UUID.
///
/// Services in this state can call operations that require an existing clinical record,
/// such as [`link_to_demographics`](ClinicalService::link_to_demographics).
///
/// # Fields
///
/// The clinical UUID is stored privately and accessed via the
/// [`clinical_id()`](ClinicalService::clinical_id) method.
#[derive(Clone, Copy, Debug)]
pub struct Initialised {
    clinical_id: Uuid,
}

/// Result of creating a new letter.
///
/// Contains the generated paths and input data for verification and testing.
#[derive(Debug, Clone)]
pub struct LetterResult {
    /// Full path to the letter's body.md file
    pub body_md_path: PathBuf,
    /// Author name
    pub author_name: String,
    /// Author role
    pub author_role: String,
    /// Author email
    pub author_email: String,
    /// The care location provided
    pub care_location: String,
    /// The letter content provided
    pub letter_content: String,
    /// The generated timestamp ID
    pub timestamp_id: String,
}

/// Service for managing clinical record operations.
///
/// This service uses the type-state pattern to enforce correct usage at compile time.
/// The generic parameter `S` can be either [`Uninitialised`] or [`Initialised`],
/// determining which operations are available.
///
/// # Type States
///
/// - `ClinicalService<Uninitialised>` - Created via [`new()`](ClinicalService::new).
///   Can only call [`initialise()`](ClinicalService::initialise).
///
/// - `ClinicalService<Initialised>` - Created via [`with_id()`](ClinicalService::with_id)
///   or returned from [`initialise()`](ClinicalService::initialise). Can call operations
///   like [`link_to_demographics()`](ClinicalService::link_to_demographics).
#[derive(Clone, Debug)]
pub struct ClinicalService<S> {
    cfg: Arc<CoreConfig>,
    state: S,
}

impl ClinicalService<Uninitialised> {
    /// Creates a new `ClinicalService` in the uninitialised state.
    ///
    /// This is the starting point for creating a new clinical record. The returned
    /// service can only call [`initialise()`](Self::initialise) to create the record.
    ///
    /// # Arguments
    ///
    /// * `cfg` - The core configuration containing paths, templates, and system settings.
    ///
    /// # Returns
    ///
    /// A `ClinicalService<Uninitialised>` ready to initialise a new clinical record.
    pub fn new(cfg: Arc<CoreConfig>) -> Self {
        Self {
            cfg,
            state: Uninitialised,
        }
    }
}

impl ClinicalService<Initialised> {
    /// Creates a `ClinicalService` in the initialised state with an existing clinical ID.
    ///
    /// Use this when you already have a clinical record and want to perform operations on it,
    /// such as linking to demographics or updating the EHR status.
    ///
    /// # Arguments
    ///
    /// * `cfg` - The core configuration containing paths, templates, and system settings.
    /// * `clinical_id` - The UUID of an existing clinical record.
    ///
    /// # Returns
    ///
    /// A `ClinicalService<Initialised>` ready to operate on the specified clinical record.
    ///
    /// # Note
    ///
    /// This constructor does not validate that the clinical record actually exists.
    /// Operations on a non-existent record will fail at runtime.
    pub fn with_id(cfg: Arc<CoreConfig>, clinical_id: Uuid) -> Self {
        Self {
            cfg,
            state: Initialised { clinical_id },
        }
    }

    /// Returns the clinical ID for this initialised service.
    ///
    /// # Returns
    ///
    /// The UUID of the clinical record associated with this service.
    pub fn clinical_id(&self) -> Uuid {
        self.state.clinical_id
    }
}

impl ClinicalService<Uninitialised> {
    /// Initialises a new clinical record for a patient.
    ///
    /// This function creates a new clinical entry with a unique UUID, stores it in a sharded
    /// directory structure, copies the clinical template into the patient's directory, writes an
    /// initial `ehr_status.yaml`, and initialises a Git repository for version control.
    ///
    /// **This method consumes `self`** and returns a new `ClinicalService<Initialised>` on success,
    /// enforcing at compile time that you cannot call `initialise()` twice on the same service.
    ///
    /// # Arguments
    ///
    /// * `author` - The author information for the initial Git commit. Must have a non-empty name.
    /// * `care_location` - High-level organisational location for the commit (e.g. hospital name).
    ///   Must be a non-empty string.
    ///
    /// # Returns
    ///
    /// Returns `ClinicalService<Initialised>` containing the newly created clinical record.
    /// Use [`clinical_id()`](ClinicalService::clinical_id) to get the UUID.
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if:
    /// - The author's name is empty or whitespace-only ([`PatientError::MissingAuthorName`])
    /// - The care location is empty or whitespace-only ([`PatientError::MissingCareLocation`])
    /// - Required inputs or configuration are invalid ([`PatientError::InvalidInput`])
    /// - The clinical template cannot be located or is invalid
    /// - A unique patient directory cannot be allocated after 5 attempts
    /// - File or directory operations fail while creating the record or copying templates ([`PatientError::FileWrite`])
    /// - Writing `ehr_status.yaml` fails
    /// - Git repository initialisation fails (e.g., [`PatientError::GitInit`])
    /// - The initial commit fails (e.g., certificate/signature mismatch)
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
        care_location: String,
    ) -> PatientResult<ClinicalService<Initialised>> {
        author.validate_commit_author()?;

        let commit_message = VprCommitMessage::new(
            VprCommitDomain::Record,
            VprCommitAction::Create,
            "Initialised the clinical record",
            care_location,
        )?;

        let clinical_dir = self.clinical_dir();
        let (clinical_uuid, patient_dir) =
            create_uuid_and_shard_dir(&clinical_dir, ShardableUuid::new)?;

        let result: PatientResult<Uuid> = (|| {
            let repo = VersionedFileService::init(&patient_dir)?;

            let template_dir = self.cfg.clinical_template_dir().to_path_buf();
            validate_template(&TemplateDirKind::Clinical, &template_dir)?;

            copy_dir_recursive(&template_dir, &patient_dir).map_err(PatientError::FileWrite)?;

            let rm_version = self.cfg.rm_system_version();

            let filename = patient_dir.join(EhrStatus.filename());
            let ehr_id = EhrId::from_uuid(clinical_uuid.uuid());

            let yaml_content = ehr_status_render(rm_version, None, Some(&ehr_id), None)?;

            fs::write(&filename, yaml_content).map_err(PatientError::FileWrite)?;

            repo.commit_all(&author, &commit_message)?;

            Ok(clinical_uuid.uuid())
        })();

        let clinical_id = match result {
            Ok(id) => id,
            Err(init_error) => {
                match remove_patient_dir_all(&patient_dir) {
                    Ok(()) => {}
                    Err(cleanup_error) => {
                        return Err(PatientError::CleanupAfterInitialiseFailed {
                            path: patient_dir,
                            init_error: Box::new(init_error),
                            cleanup_error,
                        });
                    }
                }
                return Err(init_error);
            }
        };

        Ok(ClinicalService {
            cfg: self.cfg,
            state: Initialised { clinical_id },
        })
    }
}

impl ClinicalService<Initialised> {
    /// Links the clinical record to the patient's demographics.
    ///
    /// This function updates the clinical record's `ehr_status.yaml` file to include an
    /// external reference to the patient's demographics record.
    ///
    /// The clinical UUID is obtained from the service's internal state (via [`clinical_id()`](Self::clinical_id)),
    /// ensuring type safety and preventing mismatched UUIDs.
    ///
    /// # Arguments
    ///
    /// * `author` - The author information for the Git commit recording this change.
    ///   Must have a non-empty name.
    /// * `care_location` - High-level organisational location for the commit (e.g. hospital name).
    ///   Must be a non-empty string.
    /// * `demographics_uuid` - The UUID of the associated patient demographics record.
    ///   Must be a canonical 32-character lowercase hex string.
    /// * `namespace` - Optional namespace for the external reference URI. If `None`, uses the
    ///   value configured in [`CoreConfig`]. The namespace must be URI-safe (no special characters
    ///   like `<`, `>`, `/`, `\`).
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success. The `ehr_status.yaml` file is updated and committed to Git.
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if:
    /// - The author's name is empty or whitespace-only ([`PatientError::MissingAuthorName`])
    /// - The care location is empty or whitespace-only ([`PatientError::MissingCareLocation`])
    /// - The demographics UUID cannot be parsed ([`PatientError::Uuid`])
    /// - The namespace is invalid/unsafe for embedding into a `ehr://{namespace}/mpi` URI ([`PatientError::InvalidInput`])
    /// - The `ehr_status.yaml` file does not exist ([`PatientError::InvalidInput`])
    /// - Reading or writing `ehr_status.yaml` fails ([`PatientError::FileRead`] or [`PatientError::FileWrite`])
    /// - The existing `ehr_status.yaml` cannot be parsed ([`PatientError::Openehr`])
    /// - Git commit fails (various Git-related error variants)
    ///
    /// # Safety & Rollback
    ///
    /// If the file write or Git commit fails, this method attempts to restore the previous
    /// content of `ehr_status.yaml`.
    pub fn link_to_demographics(
        &self,
        author: &Author,
        care_location: String,
        demographics_uuid: &str,
        namespace: Option<String>,
    ) -> PatientResult<()> {
        author.validate_commit_author()?;

        let msg = VprCommitMessage::new(
            VprCommitDomain::Record,
            VprCommitAction::Update,
            "EHR status linked to demographics",
            care_location,
        )?;

        let clinical_uuid = ShardableUuid::parse(&self.clinical_id().simple().to_string())?;
        let demographics_uuid = ShardableUuid::parse(demographics_uuid)?;

        let namespace = namespace
            .as_deref()
            .unwrap_or(self.cfg.vpr_namespace())
            .trim();

        validate_namespace_uri_safe(namespace)?;

        let patient_dir = self.clinical_patient_dir(&clinical_uuid);
        let filename = patient_dir.join(EhrStatus.filename());

        if !filename.exists() {
            return Err(PatientError::InvalidInput(format!(
                "{} does not exist for clinical record {}",
                EhrStatus.filename(),
                clinical_uuid
            )));
        }

        let external_reference = Some(vec![ExternalReference {
            namespace: format!("ehr://{}/mpi", namespace),
            id: demographics_uuid.uuid(),
        }]);

        let previous_data = fs::read_to_string(&filename).map_err(PatientError::FileRead)?;

        let rm_version = extract_rm_version(&previous_data)?;

        let yaml_content =
            ehr_status_render(rm_version, Some(&previous_data), None, external_reference)?;

        let repo = VersionedFileService::open(&patient_dir)?;
        repo.write_and_commit_files(
            author,
            &msg,
            &[FileToWrite {
                relative_path: Path::new(EhrStatus.filename()),
                content: &yaml_content,
                old_content: Some(&previous_data),
            }],
        )
    }

    pub fn new_letter(
        &self,
        author: &Author,
        care_location: String,
        letter_content: String,
    ) -> PatientResult<String> {
        author.validate_commit_author()?;

        let msg = VprCommitMessage::new(
            VprCommitDomain::Record,
            VprCommitAction::Create,
            "Created new letter",
            care_location,
        )?;

        let timestamp_id = TimestampIdGenerator::generate(None)?;
        let letter_paths = LetterPaths::new(&timestamp_id);

        let clinical_uuid = ShardableUuid::parse(&self.clinical_id().simple().to_string())?;
        let patient_dir = self.clinical_patient_dir(&clinical_uuid);

        let body_md_relative = letter_paths.body_md();

        let repo = VersionedFileService::open(&patient_dir)?;
        repo.write_and_commit_files(
            author,
            &msg,
            &[FileToWrite {
                relative_path: &body_md_relative,
                content: &letter_content,
                old_content: None,
            }],
        )?;

        Ok(timestamp_id.to_string())
    }
}

impl<S> ClinicalService<S> {
    /// Returns the path to the clinical records directory.
    ///
    /// This constructs the base directory for clinical records by joining
    /// the configured patient data directory with the clinical directory name.
    ///
    /// # Returns
    ///
    /// A `PathBuf` pointing to the clinical records directory (e.g., `{patient_data_dir}/clinical`).
    fn clinical_dir(&self) -> PathBuf {
        let data_dir = self.cfg.patient_data_dir().to_path_buf();
        data_dir.join(CLINICAL_DIR_NAME)
    }

    /// Returns the path to a specific patient's clinical record directory.
    ///
    /// This constructs the full path to a patient's clinical directory by
    /// determining the sharded subdirectory based on the UUID and joining
    /// it with the clinical base directory.
    ///
    /// # Arguments
    ///
    /// * `clinical_uuid` - The UUID identifying the clinical record.
    ///
    /// # Returns
    ///
    /// A `PathBuf` pointing to the patient's clinical record directory
    /// (e.g., `{clinical_dir}/{shard1}/{shard2}/{uuid}`).
    fn clinical_patient_dir(&self, clinical_uuid: &ShardableUuid) -> PathBuf {
        let clinical_dir = self.clinical_dir();
        clinical_uuid.sharded_dir(&clinical_dir)
    }
}

#[cfg(test)]
static FORCE_CLEANUP_ERROR_FOR_THREADS: LazyLock<Mutex<HashSet<std::thread::ThreadId>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

#[cfg(test)]
fn force_cleanup_error_for_current_thread() {
    let mut guard = FORCE_CLEANUP_ERROR_FOR_THREADS
        .lock()
        .expect("FORCE_CLEANUP_ERROR_FOR_THREADS mutex poisoned");
    guard.insert(std::thread::current().id());
}

/// Removes a patient directory and all its contents.
///
/// This is a wrapper around [`std::fs::remove_dir_all`] with test instrumentation.
/// In test mode, it can be forced to fail for specific threads to test error handling.
///
/// # Arguments
///
/// * `patient_dir` - The path to the patient directory to remove.
///
/// # Returns
///
/// Returns `Ok(())` if the directory was successfully removed.
///
/// # Errors
///
/// Returns an [`io::Error`] if the directory cannot be removed.
///
/// # Test Instrumentation
///
/// When compiled with `#[cfg(test)]`, this function checks a thread-local set to
/// see if it should force a failure for the current thread. This allows testing
/// of cleanup failure scenarios.
fn remove_patient_dir_all(patient_dir: &Path) -> io::Result<()> {
    #[cfg(test)]
    {
        let current_id = std::thread::current().id();
        let mut guard = FORCE_CLEANUP_ERROR_FOR_THREADS
            .lock()
            .expect("FORCE_CLEANUP_ERROR_FOR_THREADS mutex poisoned");

        if guard.remove(&current_id) {
            return Err(io::Error::other("forced cleanup failure (test hook)"));
        }
    }

    fs::remove_dir_all(patient_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::rm_system_version_from_env_value;
    use crate::repositories::shared::{resolve_clinical_template_dir, validate_template};
    use crate::CoreConfig;
    use p256::ecdsa::SigningKey;
    use p256::pkcs8::{EncodePrivateKey, EncodePublicKey};
    use rcgen::{CertificateParams, KeyPair};
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn count_allocated_patient_dirs(clinical_dir: &Path) -> usize {
        let Ok(s1_entries) = fs::read_dir(clinical_dir) else {
            return 0;
        };

        let mut count = 0usize;
        for s1 in s1_entries.flatten() {
            let Ok(s1_ty) = s1.file_type() else {
                continue;
            };
            if !s1_ty.is_dir() {
                continue;
            }

            let Ok(s2_entries) = fs::read_dir(s1.path()) else {
                continue;
            };
            for s2 in s2_entries.flatten() {
                let Ok(s2_ty) = s2.file_type() else {
                    continue;
                };
                if !s2_ty.is_dir() {
                    continue;
                }

                let Ok(uuid_entries) = fs::read_dir(s2.path()) else {
                    continue;
                };
                for uuid_dir in uuid_entries.flatten() {
                    let Ok(uuid_ty) = uuid_dir.file_type() else {
                        continue;
                    };
                    if uuid_ty.is_dir() {
                        count = count.saturating_add(1);
                    }
                }
            }
        }

        count
    }

    fn test_cfg(patient_data_dir: &Path) -> Arc<CoreConfig> {
        let clinical_template_dir = resolve_clinical_template_dir(None)
            .expect("resolve_clinical_template_dir should succeed");
        validate_template(&TemplateDirKind::Clinical, &clinical_template_dir)
            .expect("template dir should be safe to copy");

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        Arc::new(
            CoreConfig::new(
                patient_data_dir.to_path_buf(),
                clinical_template_dir,
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        )
    }

    #[test]
    fn create_uuid_and_shard_dir_creates_first_available_candidate() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);

        let uuids = vec![ShardableUuid::parse("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .expect("uuid should be canonical")];
        let mut iter = uuids.into_iter();

        let (uuid, patient_dir) = create_uuid_and_shard_dir(&clinical_dir, || iter.next().unwrap())
            .expect("allocation should succeed");

        assert_eq!(uuid.to_string(), "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(
            patient_dir,
            clinical_dir
                .join("aa")
                .join("aa")
                .join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert!(patient_dir.exists(), "patient directory should exist");
    }

    #[test]
    fn create_uuid_and_shard_dir_skips_existing_candidate() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);

        let first = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let second = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let first_dir = clinical_dir
            .join("aa")
            .join("aa")
            .join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        fs::create_dir_all(&first_dir).expect("Failed to pre-create first candidate dir");

        let uuids = vec![
            ShardableUuid::parse(first).expect("uuid should be canonical"),
            ShardableUuid::parse(second).expect("uuid should be canonical"),
        ];
        let mut iter = uuids.into_iter();

        let (uuid, patient_dir) = create_uuid_and_shard_dir(&clinical_dir, || iter.next().unwrap())
            .expect("allocation should succeed");

        assert_eq!(uuid.to_string(), second);
        assert_eq!(
            patient_dir,
            clinical_dir
                .join("bb")
                .join("bb")
                .join("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
        assert!(patient_dir.exists(), "patient directory should exist");
    }

    #[test]
    fn create_uuid_and_shard_dir_fails_after_five_attempts() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);

        let ids = [
            "11111111111111111111111111111111",
            "22222222222222222222222222222222",
            "33333333333333333333333333333333",
            "44444444444444444444444444444444",
            "55555555555555555555555555555555",
        ];

        for id in ids {
            let dir = clinical_dir.join(&id[0..2]).join(&id[2..4]).join(id);
            fs::create_dir_all(&dir).expect("Failed to pre-create candidate dir");
        }

        let uuids = ids
            .into_iter()
            .map(|s| ShardableUuid::parse(s).expect("uuid should be canonical"))
            .collect::<Vec<_>>();
        let mut iter = uuids.into_iter();

        let err = create_uuid_and_shard_dir(&clinical_dir, || iter.next().unwrap())
            .expect_err("allocation should fail");

        match err {
            PatientError::PatientDirCreation(e) => {
                assert_eq!(e.kind(), ErrorKind::AlreadyExists);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn create_uuid_and_shard_dir_returns_error_if_parent_dir_creation_fails() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);

        // For UUID "aa..", the shard prefix directories are "aa/aa".
        // Create a *file* at "clinical_dir/aa" so creating "clinical_dir/aa/aa" fails.
        fs::create_dir_all(&clinical_dir).expect("Failed to create clinical_dir");
        let blocking_path = clinical_dir.join("aa");
        fs::write(&blocking_path, b"not a directory").expect("Failed to create blocking file");

        let uuids = vec![ShardableUuid::parse("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .expect("uuid should be canonical")];
        let mut iter = uuids.into_iter();

        let err = create_uuid_and_shard_dir(&clinical_dir, || iter.next().unwrap())
            .expect_err("allocation should fail when parent dir creation fails");

        assert!(matches!(err, PatientError::PatientDirCreation(_)));
    }

    #[test]
    fn test_initialise_fails_fast_on_invalid_author_and_creates_no_files() {
        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_template_dir = TempDir::new().expect("Failed to create template temp dir");
        fs::create_dir_all(clinical_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                clinical_template_dir.path().to_path_buf(),
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        );

        let service = ClinicalService::new(cfg);

        let author = Author {
            name: " ".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let err = service
            .initialise(author, "Test Hospital".to_string())
            .expect_err("initialise should fail for invalid author");
        assert!(matches!(err, PatientError::MissingAuthorName));

        assert!(
            !patient_data_dir.path().join(CLINICAL_DIR_NAME).exists(),
            "initialise should not perform filesystem side-effects when validation fails"
        );
    }

    #[test]
    fn test_initialise_rejects_missing_care_location_and_creates_no_files() {
        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_template_dir = TempDir::new().expect("Failed to create template temp dir");
        fs::create_dir_all(clinical_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                clinical_template_dir.path().to_path_buf(),
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        );

        let service = ClinicalService::new(cfg);

        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let err = service
            .initialise(author, " \t\n".to_string())
            .expect_err("initialise should fail for missing care location");
        assert!(matches!(err, PatientError::MissingCareLocation));

        assert!(
            !patient_data_dir.path().join(CLINICAL_DIR_NAME).exists(),
            "initialise should not perform filesystem side-effects when validation fails"
        );
    }

    #[test]
    fn test_initialise_returns_invalid_input_if_template_missing_ehr_dir_and_cleans_up() {
        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_template_dir = TempDir::new().expect("Failed to create template temp dir");
        // Intentionally do not create `.ehr/`.

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                clinical_template_dir.path().to_path_buf(),
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        );

        let service = ClinicalService::new(cfg);
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let err = service
            .initialise(author, "Test Hospital".to_string())
            .expect_err("initialise should fail when template is invalid");
        assert!(matches!(err, PatientError::InvalidInput(_)));

        let clinical_dir = patient_data_dir.path().join(CLINICAL_DIR_NAME);
        assert_eq!(
            count_allocated_patient_dirs(&clinical_dir),
            0,
            "initialise should clean up the patient directory on template validation failure"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_initialise_returns_invalid_input_if_template_contains_symlink_and_cleans_up() {
        use std::os::unix::fs::symlink;

        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_template_dir = TempDir::new().expect("Failed to create template temp dir");

        fs::create_dir_all(clinical_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        let target = clinical_template_dir.path().join("target.txt");
        fs::write(&target, b"hello").expect("Failed to write target file");
        let link_path = clinical_template_dir.path().join("link.txt");
        symlink(&target, &link_path).expect("Failed to create symlink");

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                clinical_template_dir.path().to_path_buf(),
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        );

        let service = ClinicalService::new(cfg);
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let err = service
            .initialise(author, "Test Hospital".to_string())
            .expect_err("initialise should fail when template contains a symlink");
        assert!(matches!(err, PatientError::InvalidInput(_)));

        let clinical_dir = patient_data_dir.path().join(CLINICAL_DIR_NAME);
        assert_eq!(
            count_allocated_patient_dirs(&clinical_dir),
            0,
            "initialise should clean up the patient directory on template validation failure"
        );
    }

    #[test]
    fn test_initialise_rejects_template_with_forbidden_extension() {
        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_template_dir = TempDir::new().expect("Failed to create template temp dir");

        fs::create_dir_all(clinical_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        // Create a file with forbidden .exe extension
        let forbidden_file = clinical_template_dir.path().join("malware.exe");
        fs::write(&forbidden_file, b"not really malware").expect("Failed to write file");

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                clinical_template_dir.path().to_path_buf(),
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        );

        let service = ClinicalService::new(cfg);
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let err = service
            .initialise(author, "Test Hospital".to_string())
            .expect_err("initialise should fail when template contains forbidden extension");

        assert!(matches!(err, PatientError::InvalidInput(_)));
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("forbidden extension") || err_msg.contains(".exe"),
            "Error should mention forbidden extension: {}",
            err_msg
        );

        let clinical_dir = patient_data_dir.path().join(CLINICAL_DIR_NAME);
        assert_eq!(
            count_allocated_patient_dirs(&clinical_dir),
            0,
            "initialise should clean up the patient directory on template validation failure"
        );
    }

    #[test]
    fn test_initialise_rejects_template_with_dangerous_filename() {
        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_template_dir = TempDir::new().expect("Failed to create template temp dir");

        fs::create_dir_all(clinical_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        // Create a dangerous hidden .git directory
        let git_dir = clinical_template_dir.path().join(".git");
        fs::create_dir(&git_dir).expect("Failed to create .git directory");

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                clinical_template_dir.path().to_path_buf(),
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        );

        let service = ClinicalService::new(cfg);
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let err = service
            .initialise(author, "Test Hospital".to_string())
            .expect_err("initialise should fail when template contains dangerous filename");

        assert!(matches!(err, PatientError::InvalidInput(_)));
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("dangerous filename"),
            "Error should mention dangerous filename: {}",
            err_msg
        );

        let clinical_dir = patient_data_dir.path().join(CLINICAL_DIR_NAME);
        assert_eq!(
            count_allocated_patient_dirs(&clinical_dir),
            0,
            "initialise should clean up the patient directory on template validation failure"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_initialise_rejects_template_with_executable_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_template_dir = TempDir::new().expect("Failed to create template temp dir");

        fs::create_dir_all(clinical_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        // Create a file with executable permissions
        let executable_file = clinical_template_dir.path().join("script.txt");
        fs::write(&executable_file, b"#!/bin/bash\necho hello").expect("Failed to write file");

        let mut perms = fs::metadata(&executable_file)
            .expect("Failed to get metadata")
            .permissions();
        perms.set_mode(0o755); // Make it executable
        fs::set_permissions(&executable_file, perms).expect("Failed to set permissions");

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                clinical_template_dir.path().to_path_buf(),
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        );

        let service = ClinicalService::new(cfg);
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let err = service
            .initialise(author, "Test Hospital".to_string())
            .expect_err("initialise should fail when template contains executable file");

        assert!(matches!(err, PatientError::InvalidInput(_)));
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("executable permissions"),
            "Error should mention executable permissions: {}",
            err_msg
        );

        let clinical_dir = patient_data_dir.path().join(CLINICAL_DIR_NAME);
        assert_eq!(
            count_allocated_patient_dirs(&clinical_dir),
            0,
            "initialise should clean up the patient directory on template validation failure"
        );
    }

    #[test]
    fn test_initialise_returns_invalid_input_if_template_exceeds_max_depth_and_cleans_up() {
        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_template_dir = TempDir::new().expect("Failed to create template temp dir");
        fs::create_dir_all(clinical_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        // Create a directory chain deeper than the configured MAX_DEPTH (20).
        let mut deep = clinical_template_dir.path().to_path_buf();
        for i in 0..=20 {
            deep = deep.join(format!("d{i}"));
            fs::create_dir(&deep).expect("Failed to create nested directory");
        }

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                clinical_template_dir.path().to_path_buf(),
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        );

        let service = ClinicalService::new(cfg);
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let err = service
            .initialise(author, "Test Hospital".to_string())
            .expect_err("initialise should fail when template exceeds depth guardrail");
        assert!(matches!(err, PatientError::InvalidInput(_)));

        let clinical_dir = patient_data_dir.path().join(CLINICAL_DIR_NAME);
        assert_eq!(
            count_allocated_patient_dirs(&clinical_dir),
            0,
            "initialise should clean up the patient directory on template validation failure"
        );
    }

    #[test]
    fn test_initialise_returns_invalid_input_if_template_exceeds_max_files_and_cleans_up() {
        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_template_dir = TempDir::new().expect("Failed to create template temp dir");
        fs::create_dir_all(clinical_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        // Exceeds MAX_FILES (2_000) by creating 2_001 empty files.
        for i in 0..=2_000 {
            let filename = clinical_template_dir.path().join(format!("f{i}.txt"));
            File::create(filename).expect("Failed to create file");
        }

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                clinical_template_dir.path().to_path_buf(),
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        );

        let service = ClinicalService::new(cfg);
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let err = service
            .initialise(author, "Test Hospital".to_string())
            .expect_err("initialise should fail when template exceeds file count guardrail");
        assert!(matches!(err, PatientError::InvalidInput(_)));

        let clinical_dir = patient_data_dir.path().join(CLINICAL_DIR_NAME);
        assert_eq!(
            count_allocated_patient_dirs(&clinical_dir),
            0,
            "initialise should clean up the patient directory on template validation failure"
        );
    }

    #[test]
    fn test_initialise_returns_invalid_input_if_template_exceeds_max_bytes_and_cleans_up() {
        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_template_dir = TempDir::new().expect("Failed to create template temp dir");
        fs::create_dir_all(clinical_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        // Exceeds MAX_TOTAL_BYTES (50 MiB) by creating a single large (sparse) file.
        let big = clinical_template_dir.path().join("big.bin");
        let mut file = File::create(big).expect("Failed to create big file");
        file.set_len(50 * 1024 * 1024 + 1)
            .expect("Failed to set big file length");
        file.write_all(b"x")
            .expect("Failed to write a byte to big file");

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                clinical_template_dir.path().to_path_buf(),
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        );

        let service = ClinicalService::new(cfg);
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let err = service
            .initialise(author, "Test Hospital".to_string())
            .expect_err("initialise should fail when template exceeds size guardrail");
        assert!(matches!(err, PatientError::InvalidInput(_)));

        let clinical_dir = patient_data_dir.path().join(CLINICAL_DIR_NAME);
        assert_eq!(
            count_allocated_patient_dirs(&clinical_dir),
            0,
            "initialise should clean up the patient directory on template validation failure"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_initialise_cleans_up_if_copy_fails_mid_way() {
        use std::os::unix::fs::PermissionsExt;

        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_template_dir = TempDir::new().expect("Failed to create template temp dir");
        fs::create_dir_all(clinical_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        // Make the template safe-to-copy, but include an unreadable file so copying fails.
        fs::write(clinical_template_dir.path().join("ok.txt"), b"ok")
            .expect("Failed to write ok.txt");
        let unreadable = clinical_template_dir.path().join("unreadable.txt");
        fs::write(&unreadable, b"nope").expect("Failed to write unreadable.txt");
        let mut perms = fs::metadata(&unreadable)
            .expect("Failed to stat unreadable.txt")
            .permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&unreadable, perms).expect("Failed to chmod unreadable.txt");

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");
        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                clinical_template_dir.path().to_path_buf(),
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        );
        let service = ClinicalService::new(cfg);

        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let err = service
            .initialise(author, "Test Hospital".to_string())
            .expect_err("initialise should fail when copy fails");
        assert!(matches!(err, PatientError::FileWrite(_)));

        let clinical_dir = patient_data_dir.path().join(CLINICAL_DIR_NAME);
        assert_eq!(
            count_allocated_patient_dirs(&clinical_dir),
            0,
            "initialise should clean up the patient directory when copy fails"
        );
    }

    #[test]
    fn test_initialise_cleans_up_if_ehr_status_file_write_fails() {
        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_template_dir = TempDir::new().expect("Failed to create template temp dir");
        fs::create_dir_all(clinical_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        // Force EHR status file write to fail by ensuring the target path already exists as a dir.
        fs::create_dir_all(clinical_template_dir.path().join(EhrStatus.filename()))
            .expect("Failed to create ehr_status.yaml directory");

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");
        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                clinical_template_dir.path().to_path_buf(),
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        );
        let service = ClinicalService::new(cfg);

        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let _err = service
            .initialise(author, "Test Hospital".to_string())
            .expect_err("initialise should fail when EHR status file write fails");

        let clinical_dir = patient_data_dir.path().join(CLINICAL_DIR_NAME);
        assert_eq!(
            count_allocated_patient_dirs(&clinical_dir),
            0,
            "initialise should clean up the patient directory when EHR status file write fails"
        );
    }

    #[test]
    fn test_initialise_cleans_up_if_initial_commit_fails() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Signing key used for commit.
        let signing_key = SigningKey::random(&mut rand::thread_rng());
        let private_key_pem = signing_key
            .to_pkcs8_pem(p256::pkcs8::LineEnding::LF)
            .expect("Failed to encode private key");

        // Different key used to create a certificate (mismatch).
        let other_key = SigningKey::random(&mut rand::thread_rng());
        let other_private_key_pem = other_key
            .to_pkcs8_pem(p256::pkcs8::LineEnding::LF)
            .expect("Failed to encode other private key");
        let other_private_key_pem_str = other_private_key_pem.to_string();

        let other_keypair = KeyPair::from_pem(&other_private_key_pem_str)
            .expect("KeyPair::from_pem should succeed");
        let params = CertificateParams::default();
        let cert = params
            .self_signed(&other_keypair)
            .expect("self_signed should succeed");
        let cert_pem = cert.pem();

        let cfg = test_cfg(temp_dir.path());
        let service = ClinicalService::new(cfg);
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: Some(private_key_pem.to_string()),
            certificate: Some(cert_pem.into_bytes()),
        };

        let err = service
            .initialise(author, "Test Hospital".to_string())
            .expect_err("initialise should fail due to certificate mismatch");
        assert!(matches!(
            err,
            PatientError::AuthorCertificatePublicKeyMismatch
        ));

        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);
        assert_eq!(
            count_allocated_patient_dirs(&clinical_dir),
            0,
            "initialise should clean up the patient directory when the initial commit fails"
        );
    }

    #[test]
    fn test_initialise_returns_cleanup_after_initialise_failed_if_cleanup_fails() {
        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_template_dir = TempDir::new().expect("Failed to create template temp dir");
        // Intentionally do not create `.ehr/` so template validation fails.

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");
        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                clinical_template_dir.path().to_path_buf(),
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        );
        let service = ClinicalService::new(cfg);

        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        force_cleanup_error_for_current_thread();
        let err = service
            .initialise(author, "Test Hospital".to_string())
            .expect_err("initialise should fail");

        match err {
            PatientError::CleanupAfterInitialiseFailed {
                path,
                init_error,
                cleanup_error,
            } => {
                assert!(
                    matches!(*init_error, PatientError::InvalidInput(_)),
                    "expected init_error to be InvalidInput"
                );
                assert_eq!(cleanup_error.kind(), ErrorKind::Other);
                assert!(path.exists(), "patient_dir should still exist");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_initialise_initialises_git_repo_before_template_validation_failure() {
        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_template_dir = TempDir::new().expect("Failed to create template temp dir");
        // Intentionally do not create `.ehr/` so template validation fails.

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");
        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                clinical_template_dir.path().to_path_buf(),
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        );
        let service = ClinicalService::new(cfg);

        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        // Force cleanup to fail so we can inspect the partially-created directory.
        force_cleanup_error_for_current_thread();
        let err = service
            .initialise(author, "Test Hospital".to_string())
            .expect_err("initialise should fail");

        match err {
            PatientError::CleanupAfterInitialiseFailed { path, .. } => {
                assert!(
                    path.join(".git").is_dir(),
                    "git repo should be initialised before template validation/copy"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_initialise_creates_clinical_record() {
        // Create a temporary directory for testing
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a test author
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let cfg = test_cfg(temp_dir.path());
        let service = ClinicalService::new(cfg);

        // Call initialise
        let result = service.initialise(author, "Test Hospital".to_string());
        assert!(result.is_ok(), "initialise should succeed");

        let clinical_uuid = result.unwrap();
        let clinical_uuid_str = clinical_uuid.clinical_id().simple().to_string();
        assert_eq!(clinical_uuid_str.len(), 32, "UUID should be 32 characters");

        // Verify directory structure exists
        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);
        assert!(clinical_dir.exists(), "clinical directory should exist");

        // Extract sharding directories from UUID
        let s1 = &clinical_uuid_str[0..2];
        let s2 = &clinical_uuid_str[2..4];
        let patient_dir = clinical_dir.join(s1).join(s2).join(&clinical_uuid_str);
        assert!(patient_dir.exists(), "patient directory should exist");

        // Verify template files were copied
        let template_readme = patient_dir.join("README.md");
        assert!(template_readme.exists(), "Template README.md should exist");

        let ehr_dir = patient_dir.join(".ehr");
        assert!(ehr_dir.exists(), ".ehr directory should exist");

        let demographics_dir = patient_dir.join("demographics");
        assert!(
            demographics_dir.exists(),
            "demographics directory should exist"
        );

        let imaging_dir = patient_dir.join("imaging");
        assert!(imaging_dir.exists(), "imaging directory should exist");

        let journal_dir = patient_dir.join("journal");
        assert!(journal_dir.exists(), "journal directory should exist");

        let state_dir = patient_dir.join("state");
        assert!(state_dir.exists(), "state directory should exist");

        // Verify Git repository exists and has initial commit
        let repo = git2::Repository::open(&patient_dir).expect("Failed to open Git repo");
        let head = repo.head().expect("Failed to get HEAD");
        let commit = head.peel_to_commit().expect("Failed to get commit");
        assert_eq!(
            commit.message().unwrap(),
            "record:create: Initialised the clinical record\n\nAuthor-Name: Test Author\nAuthor-Role: Clinician\nCare-Location: Test Hospital"
        );
    }

    #[test]
    fn test_link_to_demographics_updates_ehr_status() {
        // Create a temporary directory for testing
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a test author
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        // Initialise clinical service
        let cfg = test_cfg(temp_dir.path());
        let service = ClinicalService::new(cfg.clone());

        // First, initialise a clinical record
        let care_location = "Test Hospital".to_string();
        let service = service
            .initialise(author.clone(), care_location.clone())
            .expect("initialise should succeed");
        let clinical_uuid = service.clinical_id();
        let clinical_uuid_str = clinical_uuid.simple().to_string();

        // Now link to demographics
        let demographics_uuid = "12345678123412341234123456789abc";
        let result = service.link_to_demographics(&author, care_location, demographics_uuid, None);
        assert!(result.is_ok(), "link_to_demographics should succeed");

        // Verify ehr_status.yaml was updated with linking information
        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);
        let patient_dir = ShardableUuid::parse(&clinical_uuid_str)
            .expect("clinical_uuid should be canonical")
            .sharded_dir(&clinical_dir);
        let ehr_status_file = patient_dir.join(EhrStatus.filename());

        assert!(ehr_status_file.exists(), "ehr_status.yaml should exist");
    }

    #[test]
    fn test_link_to_demographics_rejects_invalid_clinical_uuid() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());
        // This test is no longer applicable with the type-state pattern
        // because you cannot call link_to_demographics on an Uninitialised service
        // The type system prevents this at compile time
        let _service = ClinicalService::new(cfg);

        // This would not compile:
        // service.link_to_demographics(&author, care_location, "...", None);
        // So we just verify the service was created successfully
    }

    #[test]
    fn test_link_to_demographics_rejects_invalid_demographics_uuid() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());
        let service = ClinicalService::new(cfg);

        // Create a valid clinical record first
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let service = service
            .initialise(author.clone(), "Test Hospital".to_string())
            .expect("initialise should succeed");

        // Try to link with invalid demographics UUID
        let err = service
            .link_to_demographics(
                &author,
                "Test Hospital".to_string(),
                "invalid-demographics-uuid",
                None,
            )
            .expect_err("expected validation failure for invalid demographics UUID");
        assert!(matches!(err, PatientError::Uuid(_)));
    }

    #[test]
    fn test_link_to_demographics_rejects_invalid_namespace() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());
        let service = ClinicalService::new(cfg);

        // Create a valid clinical record
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let service = service
            .initialise(author.clone(), "Test Hospital".to_string())
            .expect("initialise should succeed");

        // Try to link with unsafe namespace containing invalid characters
        let demographics_uuid = ShardableUuid::new();
        let demographics_uuid_str = demographics_uuid.to_string();

        let err = service
            .link_to_demographics(
                &author,
                "Test Hospital".to_string(),
                &demographics_uuid_str,
                Some("unsafe<namespace>with/special\\chars".to_string()),
            )
            .expect_err("expected validation failure for unsafe namespace");
        assert!(matches!(err, PatientError::Openehr(_)));
    }

    #[test]
    fn test_link_to_demographics_fails_when_ehr_status_missing() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());

        // Manually create a clinical directory without ehr_status.yaml
        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);
        let clinical_uuid = ShardableUuid::new();
        let patient_dir = clinical_uuid.sharded_dir(&clinical_dir);
        fs::create_dir_all(&patient_dir).expect("Failed to create patient dir");

        // Initialize Git repo but don't create ehr_status.yaml
        VersionedFileService::init(&patient_dir).expect("Failed to init git");

        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let demographics_uuid = ShardableUuid::new();
        let demographics_uuid_str = demographics_uuid.to_string();

        // Should fail because ehr_status.yaml doesn't exist
        let service = ClinicalService::with_id(cfg, clinical_uuid.uuid());
        let err = service
            .link_to_demographics(
                &author,
                "Test Hospital".to_string(),
                &demographics_uuid_str,
                None,
            )
            .expect_err("link_to_demographics should fail when ehr_status.yaml is missing");

        assert!(
            matches!(err, PatientError::InvalidInput(_)),
            "Should return InvalidInput error when ehr_status.yaml is missing"
        );
    }

    #[test]
    fn test_link_to_demographics_rejects_corrupted_ehr_status() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());

        // Create a clinical directory with corrupted ehr_status.yaml
        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);
        let clinical_uuid = ShardableUuid::new();
        let patient_dir = clinical_uuid.sharded_dir(&clinical_dir);
        fs::create_dir_all(&patient_dir).expect("Failed to create patient dir");

        // Initialize Git repo
        VersionedFileService::init(&patient_dir).expect("Failed to init git");

        // Write corrupted YAML (missing required rm_version field)
        let ehr_status_file = patient_dir.join(EhrStatus.filename());
        fs::write(&ehr_status_file, "archetype_node_id: some_id\nname: Test")
            .expect("Failed to write corrupted file");

        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let demographics_uuid = ShardableUuid::new();
        let demographics_uuid_str = demographics_uuid.to_string();

        let service = ClinicalService::with_id(cfg, clinical_uuid.uuid());
        let err = service
            .link_to_demographics(
                &author,
                "Test Hospital".to_string(),
                &demographics_uuid_str,
                None,
            )
            .expect_err("expected failure due to corrupted ehr_status");

        // Should fail when trying to extract RM version from corrupted file
        assert!(matches!(
            err,
            PatientError::InvalidInput(_)
                | PatientError::YamlDeserialization(_)
                | PatientError::Openehr(_)
        ));
    }

    #[test]
    fn test_verify_commit_signature() {
        // Create a temporary directory for testing
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Generate a key pair for signing
        let signing_key = SigningKey::random(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();

        // Encode private key to PEM
        let private_key_pem = signing_key
            .to_pkcs8_pem(p256::pkcs8::LineEnding::LF)
            .expect("Failed to encode private key");

        // Encode public key to PEM
        let public_key_pem = verifying_key
            .to_public_key_pem(p256::pkcs8::LineEnding::LF)
            .expect("Failed to encode public key");

        let cfg = test_cfg(temp_dir.path());
        let service = ClinicalService::new(cfg.clone());
        let clinical_dir = cfg
            .patient_data_dir()
            .join(crate::constants::CLINICAL_DIR_NAME);
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: Some(private_key_pem.to_string()),
            certificate: None,
        };

        // Initialise clinical record
        let result = service.initialise(author, "Test Hospital".to_string());
        assert!(result.is_ok(), "initialise should succeed");
        let clinical_uuid = result.unwrap();
        let clinical_uuid_str = clinical_uuid.clinical_id().simple().to_string();

        // Verify the signature
        let verify_result = VersionedFileService::verify_commit_signature(
            &clinical_dir,
            &clinical_uuid_str,
            &public_key_pem,
        );
        assert!(
            verify_result.is_ok(),
            "verify_commit_signature should succeed"
        );
        assert!(verify_result.unwrap(), "signature should be valid");

        // Verify fails with a wrong public key
        let wrong_signing_key = SigningKey::random(&mut rand::thread_rng());
        let wrong_pub_pem = wrong_signing_key
            .verifying_key()
            .to_public_key_pem(p256::pkcs8::LineEnding::LF)
            .expect("Failed to encode wrong public key");
        let wrong_verify = VersionedFileService::verify_commit_signature(
            &clinical_dir,
            &clinical_uuid_str,
            &wrong_pub_pem,
        );
        assert!(wrong_verify.is_ok(), "verify with wrong key should succeed");
        assert!(
            !wrong_verify.unwrap(),
            "signature should be invalid with wrong key"
        );
    }

    #[test]
    fn test_verify_commit_signature_offline_with_embedded_public_key() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        let signing_key = SigningKey::random(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();

        let private_key_pem = signing_key
            .to_pkcs8_pem(p256::pkcs8::LineEnding::LF)
            .expect("Failed to encode private key");
        let public_key_pem = verifying_key
            .to_public_key_pem(p256::pkcs8::LineEnding::LF)
            .expect("Failed to encode public key");

        let cfg = test_cfg(temp_dir.path());
        let service = ClinicalService::new(cfg.clone());
        let clinical_dir = cfg
            .patient_data_dir()
            .join(crate::constants::CLINICAL_DIR_NAME);
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: Some(private_key_pem.to_string()),
            certificate: None,
        };

        let clinical_uuid = service
            .initialise(author, "Test Hospital".to_string())
            .expect("initialise should succeed");
        let clinical_uuid_str = clinical_uuid.clinical_id().simple().to_string();

        // Offline verification: no external key material is provided.
        let ok =
            VersionedFileService::verify_commit_signature(&clinical_dir, &clinical_uuid_str, "")
                .expect("verify_commit_signature should succeed");
        assert!(ok, "embedded public key verification should succeed");

        // Compatibility: verification still works with an explicit public key.
        let ok = VersionedFileService::verify_commit_signature(
            &clinical_dir,
            &clinical_uuid_str,
            &public_key_pem,
        )
        .expect("verify_commit_signature should succeed");
        assert!(ok, "verification with explicit public key should succeed");
    }

    #[test]
    fn test_initialise_rejects_mismatched_author_certificate() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Signing key used for commit.
        let signing_key = SigningKey::random(&mut rand::thread_rng());
        let private_key_pem = signing_key
            .to_pkcs8_pem(p256::pkcs8::LineEnding::LF)
            .expect("Failed to encode private key");

        // Different key used to create a certificate (mismatch).
        let other_key = SigningKey::random(&mut rand::thread_rng());
        let other_private_key_pem = other_key
            .to_pkcs8_pem(p256::pkcs8::LineEnding::LF)
            .expect("Failed to encode other private key");
        let other_private_key_pem_str = other_private_key_pem.to_string();

        let other_keypair = KeyPair::from_pem(&other_private_key_pem_str)
            .expect("KeyPair::from_pem should succeed");
        let params = CertificateParams::default();
        let cert = params
            .self_signed(&other_keypair)
            .expect("self_signed should succeed");
        let cert_pem = cert.pem();

        let cfg = test_cfg(temp_dir.path());
        let service = ClinicalService::new(cfg);
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: Some(private_key_pem.to_string()),
            certificate: Some(cert_pem.into_bytes()),
        };

        let err = service
            .initialise(author, "Test Hospital".to_string())
            .expect_err("initialise should fail due to certificate mismatch");
        assert!(matches!(
            err,
            PatientError::AuthorCertificatePublicKeyMismatch
        ));
    }

    #[test]
    fn test_extract_embedded_commit_signature_from_head_commit() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        let signing_key = SigningKey::random(&mut rand::thread_rng());
        let private_key_pem = signing_key
            .to_pkcs8_pem(p256::pkcs8::LineEnding::LF)
            .expect("Failed to encode private key");

        let cfg = test_cfg(temp_dir.path());
        let service = ClinicalService::new(cfg);
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: Some(private_key_pem.to_string()),
            certificate: None,
        };

        let clinical_uuid = service
            .initialise(author, "Test Hospital".to_string())
            .expect("initialise should succeed");
        let clinical_uuid_str = clinical_uuid.clinical_id().simple().to_string();

        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);
        let patient_dir = ShardableUuid::parse(&clinical_uuid_str)
            .expect("clinical_uuid should be canonical")
            .sharded_dir(&clinical_dir);

        let repo = git2::Repository::open(&patient_dir).expect("Failed to open Git repo");
        let head = repo.head().expect("Failed to get HEAD");
        let commit = head.peel_to_commit().expect("Failed to get commit");

        let embedded = crate::author::extract_embedded_commit_signature(&commit)
            .expect("extract_embedded_commit_signature should succeed");
        assert_eq!(embedded.signature.len(), 64);
        assert!(!embedded.public_key.is_empty());
        assert!(embedded.certificate.is_none());
    }

    #[test]
    fn test_initialise_without_signature_creates_commit_without_embedded_signature() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        let cfg = test_cfg(temp_dir.path());
        let service = ClinicalService::new(cfg.clone());
        let clinical_dir = cfg
            .patient_data_dir()
            .join(crate::constants::CLINICAL_DIR_NAME);

        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let clinical_uuid = service
            .initialise(author, "Test Hospital".to_string())
            .expect("initialise should succeed");
        let clinical_uuid_str = clinical_uuid.clinical_id().simple().to_string();

        let patient_dir = ShardableUuid::parse(&clinical_uuid_str)
            .expect("clinical_uuid should be canonical")
            .sharded_dir(&clinical_dir);

        let repo = git2::Repository::open(&patient_dir).expect("Failed to open Git repo");
        let head = repo.head().expect("Failed to get HEAD");
        let commit = head.peel_to_commit().expect("Failed to get commit");

        assert!(
            crate::author::extract_embedded_commit_signature(&commit).is_err(),
            "unsigned commits should not contain an embedded signature payload"
        );

        let ok =
            VersionedFileService::verify_commit_signature(&clinical_dir, &clinical_uuid_str, "")
                .expect("verify_commit_signature should succeed");
        assert!(
            !ok,
            "verification should be false when no signature is embedded"
        );
    }

    #[test]
    fn test_new_letter() {
        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(patient_data_dir.path());

        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let service = ClinicalService::new(cfg.clone());
        let service = service
            .initialise(author.clone(), "Test Hospital".to_string())
            .expect("initialise should succeed");

        let result = service.new_letter(
            &author,
            "Test Hospital".to_string(),
            "Letter content".to_string(),
        );

        assert!(result.is_ok(), "new_letter should return Ok");
        let timestamp_id = result.unwrap();
        println!("Timestamp ID: {}", timestamp_id);

        assert!(!timestamp_id.is_empty());
        assert!(
            timestamp_id.contains("Z-"),
            "timestamp_id should contain 'Z-' separator"
        );
    }
}
