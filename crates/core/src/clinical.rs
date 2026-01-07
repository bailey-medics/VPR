//! Patient clinical records management.
//!
//! This module handles the initialisation and management of clinical records
//! for patients.  kk

use crate::config::{validate_ehr_template_dir_safe_to_copy, CoreConfig};
use crate::constants::{CLINICAL_DIR_NAME, EHR_STATUS_FILENAME};
use crate::git::{GitService, VprCommitAction, VprCommitDomain, VprCommitMessage};
use crate::uuid::UuidService;
use crate::{
    copy_dir_recursive, extract_embedded_commit_signature, Author, PatientError, PatientResult,
};
use chrono::{DateTime, Utc};
use git2;
use openehr::{ehr_status_render, EhrId, ExternalReference};
use std::fs;
use std::io;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

#[cfg(test)]
use std::collections::HashSet;

#[cfg(test)]
use std::sync::{LazyLock, Mutex};

use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use p256::pkcs8::DecodePublicKey;
use x509_parser::prelude::*;

/// Service for managing clinical record operations.
#[derive(Clone)]
pub struct ClinicalService {
    cfg: Arc<CoreConfig>,
}

impl ClinicalService {
    pub fn new(cfg: Arc<CoreConfig>) -> Self {
        Self { cfg }
    }
}

impl ClinicalService {
    /// Initialises a new clinical record for a patient.
    ///
    /// This function creates a new clinical entry with a unique UUID, stores it in a sharded
    /// directory structure, copies the EHR template into the patient's directory, writes an
    /// initial `ehr_status.yaml`, and initialises a Git repository for version control.
    ///
    /// # Arguments
    ///
    /// * `author` - The author information for the initial Git commit.
    /// * `care_location` - High-level organisational location for the commit (e.g. hospital name).
    ///
    /// # Returns
    ///
    /// Returns the UUID of the newly created clinical record as a string (canonical form: 32
    /// lowercase hex characters, no hyphens).
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if:
    /// - required inputs or configuration are invalid (for example the EHR template cannot be
    ///   located),
    /// - a unique patient directory cannot be allocated after 5 UUID attempts,
    /// - file or directory operations fail while creating the record or copying templates,
    /// - writing `ehr_status.yaml` fails,
    /// - Git repository initialisation or the initial commit fails.
    /// - cleanup of a partially-created record directory fails.
    pub fn initialise(&self, author: Author, care_location: String) -> PatientResult<Uuid> {
        author.validate_commit_author()?;
        let msg = VprCommitMessage::new(
            VprCommitDomain::Record,
            VprCommitAction::Init,
            "Clinical record created",
            care_location,
        )?;

        let clinical_dir = self.clinical_dir(None);
        let (clinical_uuid, patient_dir) =
            allocate_unique_patient_dir(&clinical_dir, UuidService::new)?;

        let result: PatientResult<Uuid> = (|| {
            // Initialise Git repository early so failures don't leave partially-created records.
            let repo = GitService::init(&patient_dir)?;

            // Defensive guard: ensure the template directory is safe to copy.
            // This should normally be validated once at startup when `CoreConfig` is created,
            // but validating here prevents unsafe copying if an invalid config slips through.
            let template_dir = self.cfg.ehr_template_dir().to_path_buf();
            validate_ehr_template_dir_safe_to_copy(&template_dir)?;

            // Copy EHR template to patient directory
            copy_dir_recursive(&template_dir, &patient_dir).map_err(PatientError::FileWrite)?;

            let rm_version = self.cfg.rm_system_version();

            // Create initial EHR status YAML file
            let filename = patient_dir.join(EHR_STATUS_FILENAME);
            let ehr_id = EhrId::from_uuid(clinical_uuid.uuid());

            let yaml_content = ehr_status_render(rm_version, None, Some(&ehr_id), None)?;
            fs::write(&filename, yaml_content).map_err(PatientError::FileWrite)?;

            // Initial commit
            repo.commit_all(&author, &msg)?;

            Ok(clinical_uuid.uuid())
        })();

        // On error, attempt to clean up the partially-created patient directory.
        match result {
            Ok(v) => Ok(v),
            Err(init_error) => match remove_patient_dir_all(&patient_dir) {
                Ok(()) => Err(init_error),
                Err(cleanup_error) => Err(PatientError::CleanupAfterInitialiseFailed {
                    path: patient_dir,
                    init_error: Box::new(init_error),
                    cleanup_error,
                }),
            },
        }
    }

    /// Links the clinical record to the patient's demographics.
    ///
    /// This function creates an EHR status YAML file linking the clinical record
    /// to the patient's demographics via external references.
    ///
    /// # Arguments
    ///
    /// * `author` - The author information for the Git commit.
    /// * `care_location` - High-level organisational location for the commit (e.g. hospital name).
    /// * `clinical_uuid` - The UUID of the clinical record.
    /// * `demographics_uuid` - The UUID of the associated patient demographics.
    /// * `namespace` - Optional namespace for the external reference; defaults to the
    ///   value configured in `CoreConfig`.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if:
    /// - either UUID cannot be parsed,
    /// - the namespace is invalid/unsafe for embedding into a `vpr://{namespace}/mpi` URI,
    /// - writing `ehr_status.yaml` fails.
    pub fn link_to_demographics(
        &self,
        author: &Author,
        care_location: String,
        clinical_uuid: &str,
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
        let clinical_uuid = UuidService::parse(clinical_uuid)?;
        let demographics_uuid = UuidService::parse(demographics_uuid)?;

        // Use the caller-provided namespace if present; otherwise fall back to the configured
        // default. Trim and validate before embedding into a `vpr://{namespace}/mpi` URI.
        let namespace = namespace
            .as_deref()
            .unwrap_or(self.cfg.vpr_namespace())
            .trim();
        validate_namespace_safe_for_uri(namespace)?;

        let rm_version = self.cfg.rm_system_version();

        let patient_dir = self.clinical_patient_dir(&clinical_uuid, None);
        let filename = patient_dir.join(EHR_STATUS_FILENAME);

        let external_reference = Some(vec![ExternalReference {
            namespace: format!("vpr://{}/mpi", namespace),
            id: demographics_uuid.uuid(),
        }]);

        let previous_data = if filename.exists() {
            Some(fs::read_to_string(&filename).map_err(PatientError::FileRead)?)
        } else {
            None
        };

        let yaml_content = ehr_status_render(
            rm_version,
            previous_data.as_deref(),
            None,
            external_reference,
        )?;
        fs::write(&filename, yaml_content).map_err(PatientError::FileWrite)?;

        let repo = GitService::open(&patient_dir)?;
        // Pass relative path to commit_paths to avoid path canonicalization mismatches
        let relative_filename = PathBuf::from(EHR_STATUS_FILENAME);
        let commit_result =
            repo.commit_paths(author, &msg, std::slice::from_ref(&relative_filename));
        if let Err(e) = commit_result {
            // Best-effort rollback: avoid leaving uncommitted state when the commit fails.
            match previous_data {
                Some(contents) => {
                    let _ = fs::write(&filename, contents);
                }
                None => {
                    let _ = fs::remove_file(&filename);
                }
            }
            return Err(e);
        }

        Ok(())
    }

    /// Retrieves the timestamp of the first commit for a clinical record.
    ///
    /// This function opens the Git repository for the specified clinical record
    /// and returns the timestamp of the first (initial) commit.
    ///
    /// # Arguments
    ///
    /// * `clinical_uuid` - The UUID of the clinical record.
    ///
    /// # Returns
    ///
    /// Returns the timestamp of the first commit as a `DateTime<Utc>`.
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if:
    /// - the UUID cannot be parsed,
    /// - the Git repository cannot be opened or the first commit cannot be read,
    /// - the commit timestamp cannot be converted into a `DateTime<Utc>`.
    pub fn get_first_commit_time(
        &self,
        clinical_uuid: &str,
        base_dir: Option<&Path>,
    ) -> PatientResult<DateTime<Utc>> {
        let clinical_uuid = UuidService::parse(clinical_uuid)?;
        let patient_dir = self.clinical_patient_dir(&clinical_uuid, base_dir);

        let repo = GitService::open(&patient_dir)?.into_repo();
        let head = repo.head().map_err(PatientError::GitHead)?;
        let commit = head.peel_to_commit().map_err(PatientError::GitPeel)?;

        // Get the time from the commit
        let time = commit.time();
        let datetime =
            DateTime::from_timestamp(time.seconds(), 0).ok_or(PatientError::InvalidTimestamp)?;

        Ok(datetime)
    }

    /// Verifies the ECDSA signature of the latest commit in the patient's Git repository.
    ///
    /// VPR uses `git2::Repository::commit_signed` with an ECDSA P-256 signature over the
    /// *unsigned commit buffer* produced by `commit_create_buffer`.
    ///
    /// The signature, signing public key, and optional X.509 certificate are embedded directly
    /// in the commit object's `gpgsig` header as a base64-encoded JSON container.
    ///
    /// This method reconstructs the commit buffer and verifies the signature using the embedded
    /// public key, optionally checking that `public_key_pem` (if provided) matches it.
    ///
    /// # Arguments
    ///
    /// * `clinical_uuid` - The UUID of the clinical record.
    /// * `public_key_pem` - The PEM-encoded public key used for verification.
    ///
    /// # Returns
    ///
    /// Returns `true` if the signature is valid, `false` otherwise.
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if:
    /// - the UUID cannot be parsed,
    /// - the Git repository cannot be opened or the latest commit cannot be read,
    /// - `public_key_pem` is provided but cannot be parsed as a public key or X.509 certificate.
    pub fn verify_commit_signature(
        &self,
        clinical_uuid: &str,
        public_key_pem: &str,
    ) -> PatientResult<bool> {
        let clinical_uuid = UuidService::parse(clinical_uuid)?;
        let patient_dir = self.clinical_patient_dir(&clinical_uuid, None);

        let repo = GitService::open(&patient_dir)?.into_repo();

        let head = repo.head().map_err(PatientError::GitHead)?;
        let commit = head.peel_to_commit().map_err(PatientError::GitPeel)?;

        let embedded = match extract_embedded_commit_signature(&commit) {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };

        let signature = match Signature::from_slice(embedded.signature.as_slice()) {
            Ok(s) => s,
            Err(_) => return Ok(false),
        };

        let embedded_verifying_key =
            match VerifyingKey::from_sec1_bytes(embedded.public_key.as_slice()) {
                Ok(k) => k,
                Err(_) => return Ok(false),
            };

        // If a trusted key/cert was provided by the caller, it must match the embedded key.
        if !public_key_pem.trim().is_empty() {
            let trusted_key = verifying_key_from_public_key_or_cert_pem(public_key_pem)?;
            let trusted_pub_bytes = trusted_key.to_encoded_point(false).as_bytes().to_vec();
            if trusted_pub_bytes != embedded.public_key {
                return Ok(false);
            }
        }

        // Recreate the unsigned commit buffer for this commit.
        let tree = commit.tree().map_err(PatientError::GitFindTree)?;
        let parents: Vec<git2::Commit> = commit.parents().collect();
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
        let message = commit.message().unwrap_or("");
        let author = commit.author();
        let committer = commit.committer();

        let buf = repo
            .commit_create_buffer(&author, &committer, message, &tree, &parent_refs)
            .map_err(PatientError::GitCommitBuffer)?;
        let buf_str =
            String::from_utf8(buf.as_ref().to_vec()).map_err(PatientError::CommitBufferToString)?;

        // Verify with the canonical payload.
        Ok(embedded_verifying_key
            .verify(buf_str.as_bytes(), &signature)
            .is_ok())
    }
}

impl ClinicalService {
    fn clinical_dir(&self, base_dir: Option<&Path>) -> PathBuf {
        let data_dir = match base_dir {
            Some(dir) => dir.to_path_buf(),
            None => self.cfg.patient_data_dir().to_path_buf(),
        };

        data_dir.join(CLINICAL_DIR_NAME)
    }

    fn clinical_patient_dir(
        &self,
        clinical_uuid: &UuidService,
        base_dir: Option<&Path>,
    ) -> PathBuf {
        let clinical_dir = self.clinical_dir(base_dir);
        clinical_uuid.sharded_dir(&clinical_dir)
    }
}

fn validate_namespace_safe_for_uri(namespace: &str) -> PatientResult<()> {
    // The namespace is embedded into an external-reference URI: `vpr://{namespace}/mpi`.
    // Defensive guardrails:
    // - reject empty/whitespace
    // - bound size to avoid pathological inputs
    // - restrict characters to a conservative ASCII set suitable for a URI authority
    const MAX_NAMESPACE_LEN: usize = 253;

    if namespace.trim().is_empty() {
        return Err(PatientError::InvalidInput(
            "namespace cannot be empty".into(),
        ));
    }

    if namespace.len() > MAX_NAMESPACE_LEN {
        return Err(PatientError::InvalidInput(format!(
            "namespace exceeds maximum length of {} characters",
            MAX_NAMESPACE_LEN
        )));
    }

    if !namespace.is_ascii() {
        return Err(PatientError::InvalidInput(
            "namespace must contain only ASCII characters".into(),
        ));
    }

    let ok = namespace
        .bytes()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'.' | b'-' | b'_'));

    if !ok {
        return Err(PatientError::InvalidInput(
            "namespace contains invalid characters (only alphanumeric, '.', '-', '_' allowed)"
                .into(),
        ));
    }

    Ok(())
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

fn allocate_unique_patient_dir(
    clinical_dir: &Path,
    mut uuid_source: impl FnMut() -> UuidService,
) -> PatientResult<(UuidService, PathBuf)> {
    // Allocate a new UUID, but guard against pathological UUID collisions (or pre-existing
    // directories from external interference) by limiting retries.
    for _attempt in 0..5 {
        let uuid = uuid_source();
        let candidate = uuid.sharded_dir(clinical_dir);

        if candidate.exists() {
            continue;
        }

        if let Some(parent) = candidate.parent() {
            fs::create_dir_all(parent).map_err(PatientError::PatientDirCreation)?;
        }

        match fs::create_dir(&candidate) {
            Ok(()) => return Ok((uuid, candidate)),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(PatientError::PatientDirCreation(e)),
        }
    }

    Err(PatientError::PatientDirCreation(io::Error::new(
        ErrorKind::AlreadyExists,
        "failed to allocate a unique patient directory after 5 attempts",
    )))
}

fn verifying_key_from_public_key_or_cert_pem(pem_or_cert: &str) -> PatientResult<VerifyingKey> {
    if pem_or_cert.contains("-----BEGIN CERTIFICATE-----") {
        let (_, pem) = x509_parser::pem::parse_x509_pem(pem_or_cert.as_bytes())
            .map_err(|e| PatientError::EcdsaPublicKeyParse(Box::new(e)))?;
        let (_, cert) = X509Certificate::from_der(pem.contents.as_ref())
            .map_err(|e| PatientError::EcdsaPublicKeyParse(Box::new(e)))?;

        let spk = cert.public_key();
        let key_bytes = &spk.subject_public_key.data;
        VerifyingKey::from_sec1_bytes(key_bytes.as_ref())
            .map_err(|e| PatientError::EcdsaPublicKeyParse(Box::new(e)))
    } else {
        VerifyingKey::from_public_key_pem(pem_or_cert)
            .map_err(|e| PatientError::EcdsaPublicKeyParse(Box::new(e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        resolve_ehr_template_dir, rm_system_version_from_env_value,
        validate_ehr_template_dir_safe_to_copy,
    };
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
        let ehr_template_dir =
            resolve_ehr_template_dir(None).expect("resolve_ehr_template_dir should succeed");
        validate_ehr_template_dir_safe_to_copy(&ehr_template_dir)
            .expect("template dir should be safe to copy");

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        Arc::new(
            CoreConfig::new(
                patient_data_dir.to_path_buf(),
                ehr_template_dir,
                rm_system_version,
                "vpr.dev.1".into(),
            )
            .expect("CoreConfig::new should succeed"),
        )
    }

    #[test]
    fn allocate_unique_patient_dir_creates_first_available_candidate() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);

        let uuids = vec![UuidService::parse("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .expect("uuid should be canonical")];
        let mut iter = uuids.into_iter();

        let (uuid, patient_dir) =
            allocate_unique_patient_dir(&clinical_dir, || iter.next().unwrap())
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
    fn allocate_unique_patient_dir_skips_existing_candidate() {
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
            UuidService::parse(first).expect("uuid should be canonical"),
            UuidService::parse(second).expect("uuid should be canonical"),
        ];
        let mut iter = uuids.into_iter();

        let (uuid, patient_dir) =
            allocate_unique_patient_dir(&clinical_dir, || iter.next().unwrap())
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
    fn allocate_unique_patient_dir_fails_after_five_attempts() {
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
            .map(|s| UuidService::parse(s).expect("uuid should be canonical"))
            .collect::<Vec<_>>();
        let mut iter = uuids.into_iter();

        let err = allocate_unique_patient_dir(&clinical_dir, || iter.next().unwrap())
            .expect_err("allocation should fail");

        match err {
            PatientError::PatientDirCreation(e) => {
                assert_eq!(e.kind(), ErrorKind::AlreadyExists);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn allocate_unique_patient_dir_returns_error_if_parent_dir_creation_fails() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);

        // For UUID "aa..", the shard prefix directories are "aa/aa".
        // Create a *file* at "clinical_dir/aa" so creating "clinical_dir/aa/aa" fails.
        fs::create_dir_all(&clinical_dir).expect("Failed to create clinical_dir");
        let blocking_path = clinical_dir.join("aa");
        fs::write(&blocking_path, b"not a directory").expect("Failed to create blocking file");

        let uuids = vec![UuidService::parse("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .expect("uuid should be canonical")];
        let mut iter = uuids.into_iter();

        let err = allocate_unique_patient_dir(&clinical_dir, || iter.next().unwrap())
            .expect_err("allocation should fail when parent dir creation fails");

        assert!(matches!(err, PatientError::PatientDirCreation(_)));
    }

    #[test]
    fn test_initialise_fails_fast_on_invalid_author_and_creates_no_files() {
        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let ehr_template_dir = TempDir::new().expect("Failed to create template temp dir");
        fs::create_dir_all(ehr_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                ehr_template_dir.path().to_path_buf(),
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
        let ehr_template_dir = TempDir::new().expect("Failed to create template temp dir");
        fs::create_dir_all(ehr_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                ehr_template_dir.path().to_path_buf(),
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
        let ehr_template_dir = TempDir::new().expect("Failed to create template temp dir");
        // Intentionally do not create `.ehr/`.

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                ehr_template_dir.path().to_path_buf(),
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
        let ehr_template_dir = TempDir::new().expect("Failed to create template temp dir");

        fs::create_dir_all(ehr_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        let target = ehr_template_dir.path().join("target.txt");
        fs::write(&target, b"hello").expect("Failed to write target file");
        let link_path = ehr_template_dir.path().join("link.txt");
        symlink(&target, &link_path).expect("Failed to create symlink");

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                ehr_template_dir.path().to_path_buf(),
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
    fn test_initialise_returns_invalid_input_if_template_exceeds_max_depth_and_cleans_up() {
        let patient_data_dir = TempDir::new().expect("Failed to create temp dir");
        let ehr_template_dir = TempDir::new().expect("Failed to create template temp dir");
        fs::create_dir_all(ehr_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        // Create a directory chain deeper than the configured MAX_DEPTH (20).
        let mut deep = ehr_template_dir.path().to_path_buf();
        for i in 0..=20 {
            deep = deep.join(format!("d{i}"));
            fs::create_dir(&deep).expect("Failed to create nested directory");
        }

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                ehr_template_dir.path().to_path_buf(),
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
        let ehr_template_dir = TempDir::new().expect("Failed to create template temp dir");
        fs::create_dir_all(ehr_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        // Exceeds MAX_FILES (2_000) by creating 2_001 empty files.
        for i in 0..=2_000 {
            let filename = ehr_template_dir.path().join(format!("f{i}.txt"));
            File::create(filename).expect("Failed to create file");
        }

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");

        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                ehr_template_dir.path().to_path_buf(),
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
        let ehr_template_dir = TempDir::new().expect("Failed to create template temp dir");
        fs::create_dir_all(ehr_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        // Exceeds MAX_TOTAL_BYTES (50 MiB) by creating a single large (sparse) file.
        let big = ehr_template_dir.path().join("big.bin");
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
                ehr_template_dir.path().to_path_buf(),
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
        let ehr_template_dir = TempDir::new().expect("Failed to create template temp dir");
        fs::create_dir_all(ehr_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        // Make the template safe-to-copy, but include an unreadable file so copying fails.
        fs::write(ehr_template_dir.path().join("ok.txt"), b"ok").expect("Failed to write ok.txt");
        let unreadable = ehr_template_dir.path().join("unreadable.txt");
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
                ehr_template_dir.path().to_path_buf(),
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
        let ehr_template_dir = TempDir::new().expect("Failed to create template temp dir");
        fs::create_dir_all(ehr_template_dir.path().join(".ehr"))
            .expect("Failed to create .ehr directory");

        // Force EHR status file write to fail by ensuring the target path already exists as a dir.
        fs::create_dir_all(ehr_template_dir.path().join(EHR_STATUS_FILENAME))
            .expect("Failed to create ehr_status.yaml directory");

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");
        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                ehr_template_dir.path().to_path_buf(),
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
        let ehr_template_dir = TempDir::new().expect("Failed to create template temp dir");
        // Intentionally do not create `.ehr/` so template validation fails.

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");
        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                ehr_template_dir.path().to_path_buf(),
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
        let ehr_template_dir = TempDir::new().expect("Failed to create template temp dir");
        // Intentionally do not create `.ehr/` so template validation fails.

        let rm_system_version = rm_system_version_from_env_value(None)
            .expect("rm_system_version_from_env_value should succeed");
        let cfg = Arc::new(
            CoreConfig::new(
                patient_data_dir.path().to_path_buf(),
                ehr_template_dir.path().to_path_buf(),
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
        let clinical_uuid_str = clinical_uuid.simple().to_string();
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
            "record:init: Clinical record created\n\nAuthor-Name: Test Author\nAuthor-Role: Clinician\nCare-Location: Test Hospital"
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
        let service = ClinicalService::new(cfg);

        // First, initialise a clinical record
        let care_location = "Test Hospital".to_string();
        let result = service.initialise(author.clone(), care_location.clone());
        assert!(result.is_ok(), "initialise should succeed");
        let clinical_uuid = result.unwrap();
        let clinical_uuid_str = clinical_uuid.simple().to_string();

        // Now link to demographics
        let demographics_uuid = "12345678123412341234123456789abc";
        let result = service.link_to_demographics(
            &author,
            care_location,
            &clinical_uuid_str,
            demographics_uuid,
            None,
        );
        assert!(result.is_ok(), "link_to_demographics should succeed");

        // Verify ehr_status.yaml was updated with linking information
        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);
        let patient_dir = UuidService::parse(&clinical_uuid_str)
            .expect("clinical_uuid should be canonical")
            .sharded_dir(&clinical_dir);
        let ehr_status_file = patient_dir.join(EHR_STATUS_FILENAME);

        assert!(ehr_status_file.exists(), "ehr_status.yaml should exist");

        // Read and verify the content
        let content = fs::read_to_string(&ehr_status_file).expect("Failed to read ehr_status.yaml");

        let wire = openehr::read_ehr_status_yaml(&content).expect("Failed to parse openEHR YAML");
        let (_ehr_id, subject_external_refs) =
            openehr::rm_1_1_0::ehr_status::ehr_status_to_domain_parts(&wire)
                .expect("Failed to translate wire to domain parts");

        assert_eq!(wire.archetype_node_id, "openEHR-EHR-STATUS.ehr_status.v1");
        assert_eq!(wire.name.value, "EHR Status");
        assert!(wire.is_modifiable);
        assert!(wire.is_queryable);

        let expected_subject_uuid = uuid::Uuid::parse_str(demographics_uuid).expect("valid uuid");

        assert_eq!(subject_external_refs.len(), 1);
        assert_eq!(subject_external_refs[0].namespace, "vpr://vpr.dev.1/mpi");
        assert_eq!(subject_external_refs[0].id, expected_subject_uuid);
    }

    #[test]
    fn test_link_to_demographics_rejects_unsafe_namespace_without_mutation() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

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

        let care_location = "Test Hospital".to_string();
        let clinical_uuid = service
            .initialise(author.clone(), care_location.clone())
            .expect("initialise should succeed");
        let clinical_uuid_str = clinical_uuid.simple().to_string();

        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);
        let patient_dir = UuidService::parse(&clinical_uuid_str)
            .expect("clinical_uuid should be canonical")
            .sharded_dir(&clinical_dir);
        let ehr_status_file = patient_dir.join(EHR_STATUS_FILENAME);

        let before = fs::read_to_string(&ehr_status_file).expect("Failed to read ehr_status.yaml");

        let demographics_uuid = "12345678123412341234123456789abc";
        let err = service
            .link_to_demographics(
                &author,
                care_location,
                &clinical_uuid_str,
                demographics_uuid,
                Some("bad/namespace".to_string()),
            )
            .expect_err("expected validation failure");
        assert!(matches!(err, PatientError::InvalidInput(_)));

        let after = fs::read_to_string(&ehr_status_file).expect("Failed to read ehr_status.yaml");
        assert_eq!(before, after, "ehr_status.yaml should not be modified");
    }

    #[test]
    fn test_link_to_demographics_rejects_invalid_clinical_uuid() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cfg = test_cfg(temp_dir.path());
        let service = ClinicalService::new(cfg);

        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };
        let care_location = "Test Hospital".to_string();

        let err = service
            .link_to_demographics(
                &author,
                care_location,
                "not-a-canonical-uuid",
                "12345678123412341234123456789abc",
                None,
            )
            .expect_err("expected validation failure");
        assert!(matches!(err, PatientError::InvalidInput(_)));
    }

    #[test]
    fn test_get_first_commit_time() {
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
        let service = ClinicalService::new(cfg);

        // Call initialise to create a record
        let clinical_uuid = service
            .initialise(author, "Test Hospital".to_string())
            .expect("initialise should succeed");
        let clinical_uuid_str = clinical_uuid.simple().to_string();

        // Call get_first_commit_time
        let result = service.get_first_commit_time(&clinical_uuid_str, Some(temp_dir.path()));
        assert!(result.is_ok(), "get_first_commit_time should succeed");

        let timestamp = result.unwrap();
        // The timestamp should be recent (within the last minute)
        let now = Utc::now();
        let diff = now.signed_duration_since(timestamp);
        assert!(diff.num_seconds() < 60, "timestamp should be recent");
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
        let service = ClinicalService::new(cfg);
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
        let clinical_uuid_str = clinical_uuid.simple().to_string();

        // Verify the signature
        let verify_result = service.verify_commit_signature(&clinical_uuid_str, &public_key_pem);
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
        let wrong_verify = service.verify_commit_signature(&clinical_uuid_str, &wrong_pub_pem);
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
        let clinical_uuid_str = clinical_uuid.simple().to_string();

        // Offline verification: no external key material is provided.
        let ok = service
            .verify_commit_signature(&clinical_uuid_str, "")
            .expect("verify_commit_signature should succeed");
        assert!(ok, "embedded public key verification should succeed");

        // Compatibility: verification still works with an explicit public key.
        let ok = service
            .verify_commit_signature(&clinical_uuid_str, &public_key_pem)
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
        let clinical_uuid_str = clinical_uuid.simple().to_string();

        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);
        let patient_dir = UuidService::parse(&clinical_uuid_str)
            .expect("clinical_uuid should be canonical")
            .sharded_dir(&clinical_dir);

        let repo = git2::Repository::open(&patient_dir).expect("Failed to open Git repo");
        let head = repo.head().expect("Failed to get HEAD");
        let commit = head.peel_to_commit().expect("Failed to get commit");

        let embedded = crate::extract_embedded_commit_signature(&commit)
            .expect("extract_embedded_commit_signature should succeed");
        assert_eq!(embedded.signature.len(), 64);
        assert!(!embedded.public_key.is_empty());
        assert!(embedded.certificate.is_none());
    }

    #[test]
    fn test_initialise_without_signature_creates_commit_without_embedded_signature() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        let cfg = test_cfg(temp_dir.path());
        let service = ClinicalService::new(cfg);

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
        let clinical_uuid_str = clinical_uuid.simple().to_string();

        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);
        let patient_dir = UuidService::parse(&clinical_uuid_str)
            .expect("clinical_uuid should be canonical")
            .sharded_dir(&clinical_dir);

        let repo = git2::Repository::open(&patient_dir).expect("Failed to open Git repo");
        let head = repo.head().expect("Failed to get HEAD");
        let commit = head.peel_to_commit().expect("Failed to get commit");

        assert!(
            crate::extract_embedded_commit_signature(&commit).is_err(),
            "unsigned commits should not contain an embedded signature payload"
        );

        let ok = service
            .verify_commit_signature(&clinical_uuid_str, "")
            .expect("verify_commit_signature should succeed");
        assert!(
            !ok,
            "verification should be false when no signature is embedded"
        );
    }
}
