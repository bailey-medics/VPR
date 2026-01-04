//! Patient clinical records management.
//!
//! This module handles the initialisation and management of clinical records
//! for patients.

use crate::constants::{
    CLINICAL_DIR_NAME, DEFAULT_PATIENT_DATA_DIR, EHR_STATUS_FILENAME, EHR_TEMPLATE_DIR, LATEST_RM,
};
use crate::git::{GitService, VprCommitAction, VprCommitDomain, VprCommitMessage};
use crate::uuid::UuidService;
use crate::{clinical_data_path, Author, PatientError, PatientResult};
use chrono::{DateTime, Utc};
use git2;
use std::fs;
use std::io;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use p256::pkcs8::DecodePublicKey;
use x509_parser::prelude::*;

/// Service for managing clinical record operations.
#[derive(Default, Clone)]
pub struct ClinicalService;

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
    pub fn initialise(&self, author: Author, care_location: String) -> PatientResult<String> {
        // Preflight checks before any filesystem side-effects.
        let rm_version = rm_version_from_env_or_latest()?;
        let template_dir = resolve_ehr_template_dir()?;
        validate_ehr_template_dir_safe_to_copy(&template_dir)?;

        let clinical_dir = clinical_data_path();
        let mut clinical_uuid_allocating: Option<UuidService> = None;
        let mut patient_dir_allocating: Option<PathBuf> = None;

        // Allocate a new UUID, but guard against pathological UUID collisions (or pre-existing
        // directories from external interference) by limiting retries.
        for _attempt in 0..5 {
            let uuid = UuidService::new();
            let candidate = uuid.sharded_dir(&clinical_dir);

            if candidate.exists() {
                continue;
            }

            if let Some(parent) = candidate.parent() {
                fs::create_dir_all(parent).map_err(PatientError::PatientDirCreation)?;
            }

            match fs::create_dir(&candidate) {
                Ok(()) => {
                    clinical_uuid_allocating = Some(uuid);
                    patient_dir_allocating = Some(candidate);
                    break;
                }
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(PatientError::PatientDirCreation(e)),
            }
        }

        let clinical_uuid = clinical_uuid_allocating.ok_or_else(|| {
            PatientError::PatientDirCreation(io::Error::new(
                ErrorKind::AlreadyExists,
                "failed to allocate a unique patient directory after 5 attempts",
            ))
        })?;

        let patient_dir =
            patient_dir_allocating.expect("patient_dir must be set when clinical_uuid is set");

        let result: PatientResult<String> = (|| {
            // Initialise Git repository early so failures don't leave partially-created records.
            let repo = GitService::init(&patient_dir)?;

            // Copy EHR template to patient directory
            copy_dir_recursive(&template_dir, &patient_dir).map_err(PatientError::FileWrite)?;

            // Create initial EHR status YAML file
            let filename = patient_dir.join(EHR_STATUS_FILENAME);
            openehr::ehr_status_write(rm_version, &filename, clinical_uuid.uuid(), None)?;

            // Initial commit
            let msg = VprCommitMessage::new(
                VprCommitDomain::Record,
                VprCommitAction::Init,
                "Clinical record created",
                care_location,
            )?;
            repo.commit_all(&author, &msg)?;

            Ok(clinical_uuid.into_string())
        })();

        if result.is_err() {
            let _ = fs::remove_dir_all(&patient_dir);
        }

        result
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
    /// Returns a `PatientError` if:
    /// - either UUID cannot be parsed,
    /// - writing `ehr_status.yaml` fails.
    pub fn link_to_demographics(
        &self,
        clinical_uuid: &str,
        demographics_uuid: &str,
        namespace: Option<String>,
    ) -> PatientResult<()> {
        let rm_version = rm_version_from_env_or_latest()?;

        let namespace = namespace.unwrap_or_else(|| {
            std::env::var("VPR_NAMESPACE").unwrap_or_else(|_| "vpr.dev.1".into())
        });

        let clinical_dir = clinical_data_path();

        let clinical_uuid = UuidService::parse(clinical_uuid)?;
        let patient_dir = clinical_uuid.sharded_dir(&clinical_dir);

        let filename = patient_dir.join(EHR_STATUS_FILENAME);

        // Create updated EHR status with linking information
        let subject_id =
            uuid::Uuid::parse_str(demographics_uuid).map_err(|_| PatientError::InvalidInput)?;

        let subject = Some(vec![openehr::SubjectExternalRef {
            namespace: format!("vpr://{}/mpi", namespace),
            id: subject_id,
        }]);
        openehr::ehr_status_write(rm_version, &filename, clinical_uuid.uuid(), subject)?;

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
        let base = base_dir
            .map(|p| p.to_string_lossy().to_string())
            .or_else(|| std::env::var("PATIENT_DATA_DIR").ok())
            .unwrap_or_else(|| DEFAULT_PATIENT_DATA_DIR.into());
        let data_dir = Path::new(&base);
        let clinical_dir = data_dir.join(CLINICAL_DIR_NAME);

        let clinical_uuid = UuidService::parse(clinical_uuid)?;
        let patient_dir = clinical_uuid.sharded_dir(&clinical_dir);

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
        let clinical_dir = clinical_data_path();

        let clinical_uuid = UuidService::parse(clinical_uuid)?;
        let patient_dir = clinical_uuid.sharded_dir(&clinical_dir);

        let repo = GitService::open(&patient_dir)?.into_repo();

        let head = repo.head().map_err(PatientError::GitHead)?;
        let commit = head.peel_to_commit().map_err(PatientError::GitPeel)?;

        let embedded = match crate::extract_embedded_commit_signature(&commit) {
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

fn rm_version_from_env_or_latest() -> PatientResult<openehr::RmVersion> {
    let version = std::env::var("RM_SYSTEM_VERSION")
        .ok()
        .map(|v| v.parse::<openehr::RmVersion>())
        .transpose()?;

    Ok(version.unwrap_or(LATEST_RM))
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

/// Recursively copy a directory and its contents to a destination
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !src.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Source directory does not exist: {}", src.display()),
        ));
    }

    // Create destination directory if it doesn't exist
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let entry_path = entry.path();
        let file_name = entry.file_name();

        let dest_path = dst.join(file_name);

        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &dest_path)?;
        } else {
            fs::copy(&entry_path, &dest_path)?;
        }
    }

    Ok(())
}

fn resolve_ehr_template_dir() -> PatientResult<PathBuf> {
    fn looks_like_template_dir(path: &Path) -> bool {
        path.join(".ehr").is_dir()
    }

    if let Ok(path) = std::env::var("VPR_EHR_TEMPLATE_DIR") {
        let template_dir = PathBuf::from(path);
        if template_dir.is_dir() && looks_like_template_dir(&template_dir) {
            return Ok(template_dir);
        }
        return Err(PatientError::InvalidInput);
    }

    let cwd_relative = PathBuf::from(EHR_TEMPLATE_DIR);
    if cwd_relative.is_dir() && looks_like_template_dir(&cwd_relative) {
        return Ok(cwd_relative);
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest_dir.ancestors() {
        let candidate = ancestor.join(EHR_TEMPLATE_DIR);
        if candidate.is_dir() && looks_like_template_dir(&candidate) {
            return Ok(candidate);
        }
    }

    Err(PatientError::InvalidInput)
}

fn validate_ehr_template_dir_safe_to_copy(template_dir: &Path) -> PatientResult<()> {
    // Guardrails for environment overrides without hardcoding expected folder names.
    //
    // Goals:
    // - allow templates to evolve (e.g. add bloods/) without code changes
    // - prevent accidental "copy the world" when VPR_EHR_TEMPLATE_DIR is set to something broad
    // - avoid copying unsafe filesystem entries like symlinks or device files

    const MAX_FILES: usize = 2_000;
    const MAX_TOTAL_BYTES: u64 = 50 * 1024 * 1024; // 50 MiB
    const MAX_DEPTH: usize = 20;

    fn scan_dir(
        path: &Path,
        depth: usize,
        files: &mut usize,
        bytes: &mut u64,
    ) -> PatientResult<()> {
        if depth > MAX_DEPTH {
            return Err(PatientError::InvalidInput);
        }

        for entry in fs::read_dir(path).map_err(PatientError::FileRead)? {
            let entry = entry.map_err(PatientError::FileRead)?;
            let entry_path = entry.path();
            let metadata = fs::symlink_metadata(&entry_path).map_err(PatientError::FileRead)?;
            let file_type = metadata.file_type();

            if file_type.is_symlink() {
                return Err(PatientError::InvalidInput);
            }

            if file_type.is_file() {
                *files = files.saturating_add(1);
                *bytes = bytes.saturating_add(metadata.len());

                if *files > MAX_FILES || *bytes > MAX_TOTAL_BYTES {
                    return Err(PatientError::InvalidInput);
                }
            } else if file_type.is_dir() {
                scan_dir(&entry_path, depth + 1, files, bytes)?;
            } else {
                // Reject special files (devices, fifos, sockets, etc).
                return Err(PatientError::InvalidInput);
            }
        }

        Ok(())
    }

    // Minimal sanity check: templates must at least contain the hidden .ehr folder.
    // This prevents common foot-guns like VPR_EHR_TEMPLATE_DIR=".".
    if !template_dir.join(".ehr").is_dir() {
        return Err(PatientError::InvalidInput);
    }

    let mut files = 0usize;
    let mut bytes = 0u64;
    scan_dir(template_dir, 0, &mut files, &mut bytes)
}

/// Recursively add all files in a directory to a Git index
#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::SigningKey;
    use p256::pkcs8::{EncodePrivateKey, EncodePublicKey};
    use rcgen::{CertificateParams, KeyPair};
    use std::env;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("lock poisoned")
    }

    #[test]
    fn test_initialise_creates_clinical_record() {
        let _lock = env_lock();
        // Save original env var
        let original_env = env::var("PATIENT_DATA_DIR").ok();

        // Create a temporary directory for testing
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let temp_path = temp_dir.path().to_str().unwrap();

        // Set PATIENT_DATA_DIR to the temp directory
        env::set_var("PATIENT_DATA_DIR", temp_path);

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
        let service = ClinicalService;

        // Call initialise
        let result = service.initialise(author, "Test Hospital".to_string());
        assert!(result.is_ok(), "initialise should succeed");

        let clinical_uuid = result.unwrap();
        assert_eq!(clinical_uuid.len(), 32, "UUID should be 32 characters");

        // Verify directory structure exists
        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);
        assert!(clinical_dir.exists(), "clinical directory should exist");

        // Extract sharding directories from UUID
        let s1 = &clinical_uuid[0..2];
        let s2 = &clinical_uuid[2..4];
        let patient_dir = clinical_dir.join(s1).join(s2).join(&clinical_uuid);
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

        // Clean up environment variable
        env::remove_var("PATIENT_DATA_DIR");
        if let Some(original) = original_env {
            env::set_var("PATIENT_DATA_DIR", original);
        }
    }

    #[test]
    fn test_link_to_demographics_updates_ehr_status() {
        let _lock = env_lock();
        // Save original env var
        let original_env = env::var("PATIENT_DATA_DIR").ok();

        // Create a temporary directory for testing
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let temp_path = temp_dir.path().to_str().unwrap();

        // Set PATIENT_DATA_DIR to the temp directory
        env::set_var("PATIENT_DATA_DIR", temp_path);

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
        let service = ClinicalService;

        // First, initialise a clinical record
        let result = service.initialise(author, "Test Hospital".to_string());
        assert!(result.is_ok(), "initialise should succeed");
        let clinical_uuid = result.unwrap();

        // Now link to demographics
        let demographics_uuid = "12345678-1234-1234-1234-123456789abc";
        let result = service.link_to_demographics(&clinical_uuid, demographics_uuid, None);
        assert!(result.is_ok(), "link_to_demographics should succeed");

        // Verify ehr_status.yaml was updated with linking information
        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);
        let patient_dir = UuidService::parse(&clinical_uuid)
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

        // Clean up environment variable
        env::remove_var("PATIENT_DATA_DIR");
        if let Some(original) = original_env {
            env::set_var("PATIENT_DATA_DIR", original);
        }
    }

    #[test]
    fn test_get_first_commit_time() {
        let _lock = env_lock();
        // Save original env var
        let original_env = env::var("PATIENT_DATA_DIR").ok();

        // Create a temporary directory for testing
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let temp_path = temp_dir.path().to_str().unwrap();

        // Set PATIENT_DATA_DIR to the temp directory
        env::set_var("PATIENT_DATA_DIR", temp_path);

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
        let service = ClinicalService;

        // Call initialise to create a record
        let clinical_uuid = service
            .initialise(author, "Test Hospital".to_string())
            .expect("initialise should succeed");

        // Call get_first_commit_time
        let result = service.get_first_commit_time(&clinical_uuid, Some(temp_dir.path()));
        assert!(result.is_ok(), "get_first_commit_time should succeed");

        let timestamp = result.unwrap();
        // The timestamp should be recent (within the last minute)
        let now = Utc::now();
        let diff = now.signed_duration_since(timestamp);
        assert!(diff.num_seconds() < 60, "timestamp should be recent");

        // Clean up environment variable
        env::remove_var("PATIENT_DATA_DIR");
        if let Some(original) = original_env {
            env::set_var("PATIENT_DATA_DIR", original);
        }
    }

    #[test]
    fn test_verify_commit_signature() {
        let _lock = env_lock();
        // Save original env var
        let original_env = env::var("PATIENT_DATA_DIR").ok();

        // Create a temporary directory for testing
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let temp_path = temp_dir.path().to_str().unwrap();
        env::set_var("PATIENT_DATA_DIR", temp_path);

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

        let service = ClinicalService;
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

        // Verify the signature
        let verify_result = service.verify_commit_signature(&clinical_uuid, &public_key_pem);
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
        let wrong_verify = service.verify_commit_signature(&clinical_uuid, &wrong_pub_pem);
        assert!(wrong_verify.is_ok(), "verify with wrong key should succeed");
        assert!(
            !wrong_verify.unwrap(),
            "signature should be invalid with wrong key"
        );

        // Clean up environment variable
        env::remove_var("PATIENT_DATA_DIR");
        if let Some(original) = original_env {
            env::set_var("PATIENT_DATA_DIR", original);
        }
    }

    #[test]
    fn test_verify_commit_signature_offline_with_embedded_public_key() {
        let _lock = env_lock();
        let original_env = env::var("PATIENT_DATA_DIR").ok();

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let temp_path = temp_dir.path().to_str().unwrap();
        env::set_var("PATIENT_DATA_DIR", temp_path);

        let signing_key = SigningKey::random(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();

        let private_key_pem = signing_key
            .to_pkcs8_pem(p256::pkcs8::LineEnding::LF)
            .expect("Failed to encode private key");
        let public_key_pem = verifying_key
            .to_public_key_pem(p256::pkcs8::LineEnding::LF)
            .expect("Failed to encode public key");

        let service = ClinicalService;
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

        // Offline verification: no external key material is provided.
        let ok = service
            .verify_commit_signature(&clinical_uuid, "")
            .expect("verify_commit_signature should succeed");
        assert!(ok, "embedded public key verification should succeed");

        // Compatibility: verification still works with an explicit public key.
        let ok = service
            .verify_commit_signature(&clinical_uuid, &public_key_pem)
            .expect("verify_commit_signature should succeed");
        assert!(ok, "verification with explicit public key should succeed");

        env::remove_var("PATIENT_DATA_DIR");
        if let Some(original) = original_env {
            env::set_var("PATIENT_DATA_DIR", original);
        }
    }

    #[test]
    fn test_initialise_rejects_mismatched_author_certificate() {
        let _lock = env_lock();
        let original_env = env::var("PATIENT_DATA_DIR").ok();

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let temp_path = temp_dir.path().to_str().unwrap();
        env::set_var("PATIENT_DATA_DIR", temp_path);

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

        let service = ClinicalService;
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

        env::remove_var("PATIENT_DATA_DIR");
        if let Some(original) = original_env {
            env::set_var("PATIENT_DATA_DIR", original);
        }
    }

    #[test]
    fn test_extract_embedded_commit_signature_from_head_commit() {
        let _lock = env_lock();
        let original_env = env::var("PATIENT_DATA_DIR").ok();

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let temp_path = temp_dir.path().to_str().unwrap();
        env::set_var("PATIENT_DATA_DIR", temp_path);

        let signing_key = SigningKey::random(&mut rand::thread_rng());
        let private_key_pem = signing_key
            .to_pkcs8_pem(p256::pkcs8::LineEnding::LF)
            .expect("Failed to encode private key");

        let service = ClinicalService;
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

        let clinical_dir = temp_dir.path().join(CLINICAL_DIR_NAME);
        let patient_dir = UuidService::parse(&clinical_uuid)
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

        env::remove_var("PATIENT_DATA_DIR");
        if let Some(original) = original_env {
            env::set_var("PATIENT_DATA_DIR", original);
        }
    }
}
