//! Patient clinical records management.
//!
//! This module handles the initialisation and management of clinical records
//! for patients. It includes creating clinical entries with timestamps,
//! initialising Git repositories for version control, and writing EHR status
//! information in YAML format.

use crate::Author;
use chrono::{DateTime, Utc};
use git2;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::fs;
use std::path::Path;
use uuid::Uuid;

use base64::{engine::general_purpose, Engine as _};
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use p256::pkcs8::DecodePublicKey;
use x509_parser::prelude::*;

use crate::{PatientError, PatientResult};

/// Represents the EHR status information in openEHR format.
/// This struct models the EHR status archetype for patient records.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct EhrStatus {
    ehr_id: EhrId,
    #[serde(skip_serializing_if = "Option::is_none")]
    archetype_node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<Name>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_modifiable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_queryable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subject: Option<Subject>,
}

/// Represents a name value in the EHR status.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Name {
    value: String,
}

/// Represents the subject of the EHR status, linking to the patient.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Subject {
    external_ref: ExternalRef,
}

/// Represents an external reference to the patient in the EHR system.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct ExternalRef {
    namespace: String,
    #[serde(rename = "type")]
    type_: String,
    id: String,
}

/// Represents the initial EHR status with just the EHR ID.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct EhrStatusInit {
    ehr_id: EhrId,
}

/// Represents the EHR ID value.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct EhrId {
    value: String,
}

/// Service for managing clinical record operations.
#[derive(Default, Clone)]
pub struct ClinicalService;

impl ClinicalService {
    /// Initialises a new clinical record for a patient.
    ///
    /// This function creates a new clinical entry with a unique UUID, stores it
    /// in a JSON file within a sharded directory structure, and initialises a
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
    /// Returns a `PatientError` if any step in the initialisation fails, such as
    /// directory creation, file writing, or Git operations.
    pub fn initialise(&self, author: Author) -> PatientResult<String> {
        // Generate 32-hex form UUID without hyphens and in lowercase safe for directory naming
        let uuid = Uuid::new_v4().simple().to_string();

        // Shard UUID into two-level hex dirs from first 4 chars of the 32-char uuid
        let s1 = &uuid[0..2];
        let s2 = &uuid[2..4];
        let patient_dir = crate::clinical_data_path().join(s1).join(s2).join(&uuid);
        fs::create_dir_all(&patient_dir).map_err(PatientError::PatientDirCreation)?;

        // Copy EHR template to patient directory
        // 1st navigate from core crate directory to workspace root
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").to_string());
        let workspace_root = Path::new(&manifest_dir)
            .parent()
            .and_then(|p| p.parent())
            .ok_or(PatientError::InvalidInput)?;
        let template_dir = workspace_root.join(crate::constants::EHR_TEMPLATE_DIR);
        copy_dir_recursive(&template_dir, &patient_dir).map_err(PatientError::FileWrite)?;

        // Create initial EHR status YAML file
        let filename = patient_dir.join(crate::constants::EHR_STATUS_FILENAME);
        let ehr_status = EhrStatusInit {
            ehr_id: EhrId {
                value: uuid.clone(),
            },
        };
        crate::yaml_write(&filename, &ehr_status)?;

        // Initialise Git repository for the patient
        let repo = git2::Repository::init(&patient_dir).map_err(PatientError::GitInit)?;

        crate::git::commit_all(&repo, &patient_dir, &author, "Initial clinical record")?;

        Ok(uuid)
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
        uuid: &str,
        demographics_uuid: &str,
        namespace: Option<String>,
    ) -> PatientResult<()> {
        let namespace = namespace.unwrap_or_else(|| {
            std::env::var("VPR_NAMESPACE").unwrap_or_else(|_| "vpr.dev.1".into())
        });

        let clinical_dir = crate::clinical_data_path();

        let uuid = uuid.to_lowercase();
        let s1 = &uuid[0..2];
        let s2 = &uuid[2..4];
        let patient_dir = clinical_dir.join(s1).join(s2).join(&uuid);

        // Read existing EHR status to get the current ehr_id
        let filename = patient_dir.join(crate::constants::EHR_STATUS_FILENAME);
        let existing_yaml = fs::read_to_string(&filename).map_err(PatientError::FileRead)?;
        let existing_status: EhrStatusInit =
            serde_yaml::from_str(&existing_yaml).map_err(PatientError::YamlDeserialization)?;

        // Create updated EHR status with linking information
        let ehr_status = EhrStatus {
            ehr_id: existing_status.ehr_id,
            archetype_node_id: Some("openEHR-EHR-STATUS.ehr_status.v1".to_string()),
            name: Some(Name {
                value: "EHR Status".to_string(),
            }),
            is_modifiable: Some(true),
            is_queryable: Some(true),
            subject: Some(Subject {
                external_ref: ExternalRef {
                    namespace: format!("vpr://{}/mpi", namespace),
                    type_: "PERSON".to_string(),
                    id: demographics_uuid.to_string(),
                },
            }),
        };

        // Write the updated EHR status
        crate::yaml_write(&filename, &ehr_status)?;

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
    /// # Arguments
    ///
    /// * `uuid` - The UUID of the clinical record.
    /// * `base_dir` - Optional base directory for patient data; defaults to PATIENT_DATA_DIR env var or "patient_data".
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if the repository cannot be opened, the head
    /// cannot be retrieved, or the commit cannot be peeled.
    pub fn get_first_commit_time(
        &self,
        uuid: &str,
        base_dir: Option<&Path>,
    ) -> PatientResult<DateTime<Utc>> {
        let base = base_dir
            .map(|p| p.to_string_lossy().to_string())
            .or_else(|| std::env::var("PATIENT_DATA_DIR").ok())
            .unwrap_or_else(|| crate::constants::DEFAULT_PATIENT_DATA_DIR.into());
        let data_dir = Path::new(&base);
        let clinical_dir = data_dir.join(crate::constants::CLINICAL_DIR_NAME);

        let uuid = uuid.to_lowercase();
        let s1 = &uuid[0..2];
        let s2 = &uuid[2..4];
        let patient_dir = clinical_dir.join(s1).join(s2).join(&uuid);

        let repo = git2::Repository::open(&patient_dir).map_err(PatientError::GitOpen)?;
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
    /// VPR uses `git2::Repository::commit_signed` with an ECDSA P-256 signature encoded as
    /// base64 over the *unsigned commit buffer* produced by `commit_create_buffer`.
    /// This method reconstructs that buffer and verifies the signature using `public_key_pem`.
    ///
    /// # Arguments
    ///
    /// * `uuid` - The UUID of the clinical record.
    /// * `public_key_pem` - The PEM-encoded public key used for verification.
    ///
    /// # Returns
    ///
    /// Returns `true` if the signature is valid, `false` otherwise.
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if the repository cannot be opened, the commit cannot be accessed,
    /// or the signature/public key cannot be parsed.
    pub fn verify_commit_signature(&self, uuid: &str, public_key_pem: &str) -> PatientResult<bool> {
        let clinical_dir = crate::clinical_data_path();
        let uuid = uuid.to_lowercase();
        let s1 = &uuid[0..2];
        let s2 = &uuid[2..4];
        let patient_dir = clinical_dir.join(s1).join(s2).join(&uuid);

        let repo = git2::Repository::open(&patient_dir).map_err(PatientError::GitOpen)?;

        let head = repo.head().map_err(PatientError::GitHead)?;
        let commit = head.peel_to_commit().map_err(PatientError::GitPeel)?;

        // Extract signature from the commit header (written by `commit_signed`).
        let sig_field = match commit.header_field_bytes("gpgsig") {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };
        if sig_field.is_empty() {
            return Ok(false);
        }

        let sig_field_str = match std::str::from_utf8(sig_field.as_ref()) {
            Ok(s) => s,
            Err(_) => return Ok(false),
        };
        // The `gpgsig` value may be wrapped/indented; normalise by stripping whitespace.
        let sig_b64: String = sig_field_str.lines().map(|l| l.trim()).collect();

        let sig_bytes = match general_purpose::STANDARD.decode(sig_b64) {
            Ok(b) => b,
            Err(_) => return Ok(false),
        };
        let signature = match Signature::from_slice(&sig_bytes) {
            Ok(s) => s,
            Err(_) => return Ok(false),
        };

        let verifying_key = verifying_key_from_public_key_or_cert_pem(public_key_pem)?;

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

        // Verify with the correct payload.
        if verifying_key.verify(buf_str.as_bytes(), &signature).is_ok() {
            return Ok(true);
        }

        // Backwards compatibility: older code in this repo signed the Git object framing too.
        let legacy_payload = format!("commit {}\0{}", buf_str.len(), buf_str);
        Ok(verifying_key
            .verify(legacy_payload.as_bytes(), &signature)
            .is_ok())
    }
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

/// Recursively add all files in a directory to a Git index
#[cfg(test)]
mod tests {
    use super::*;
    use p256::pkcs8::{EncodePrivateKey, EncodePublicKey};
    use std::env;
    use tempfile::TempDir;

    #[test]
    fn test_initialise_creates_clinical_record() {
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
            email: "test@example.com".to_string(),
            signature: None,
        };

        // Initialise clinical service
        let service = ClinicalService;

        // Call initialise
        let result = service.initialise(author);
        assert!(result.is_ok(), "initialise should succeed");

        let clinical_uuid = result.unwrap();
        assert_eq!(clinical_uuid.len(), 32, "UUID should be 32 characters");

        // Verify directory structure exists
        let clinical_dir = temp_dir.path().join(crate::constants::CLINICAL_DIR_NAME);
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
        assert_eq!(commit.message().unwrap(), "Initial clinical record");

        // Clean up environment variable
        env::remove_var("PATIENT_DATA_DIR");
        if let Some(original) = original_env {
            env::set_var("PATIENT_DATA_DIR", original);
        }
    }

    #[test]
    fn test_link_to_demographics_updates_ehr_status() {
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
            email: "test@example.com".to_string(),
            signature: None,
        };

        // Initialise clinical service
        let service = ClinicalService;

        // First, initialise a clinical record
        let result = service.initialise(author);
        assert!(result.is_ok(), "initialise should succeed");
        let clinical_uuid = result.unwrap();

        // Now link to demographics
        let demographics_uuid = "12345678-1234-1234-1234-123456789abc";
        let result = service.link_to_demographics(&clinical_uuid, demographics_uuid, None);
        assert!(result.is_ok(), "link_to_demographics should succeed");

        // Verify ehr_status.yaml was updated with linking information
        let clinical_dir = temp_dir.path().join(crate::constants::CLINICAL_DIR_NAME);
        let id_lower = clinical_uuid.to_lowercase();
        let s1 = &id_lower[0..2];
        let s2 = &id_lower[2..4];
        let patient_dir = clinical_dir.join(s1).join(s2).join(&id_lower);
        let ehr_status_file = patient_dir.join(crate::constants::EHR_STATUS_FILENAME);

        assert!(ehr_status_file.exists(), "ehr_status.yaml should exist");

        // Read and verify the content
        let content = fs::read_to_string(&ehr_status_file).expect("Failed to read ehr_status.yaml");
        let ehr_status: EhrStatus = serde_yaml::from_str(&content).expect("Failed to parse YAML");

        // Check that the linking information was added
        assert_eq!(
            ehr_status.archetype_node_id,
            Some("openEHR-EHR-STATUS.ehr_status.v1".to_string())
        );
        assert_eq!(
            ehr_status.name,
            Some(Name {
                value: "EHR Status".to_string()
            })
        );
        assert_eq!(ehr_status.is_modifiable, Some(true));
        assert_eq!(ehr_status.is_queryable, Some(true));
        assert_eq!(
            ehr_status.subject,
            Some(Subject {
                external_ref: ExternalRef {
                    namespace: "vpr://vpr.dev.1/mpi".to_string(),
                    type_: "PERSON".to_string(),
                    id: demographics_uuid.to_string(),
                }
            })
        );

        // Clean up environment variable
        env::remove_var("PATIENT_DATA_DIR");
        if let Some(original) = original_env {
            env::set_var("PATIENT_DATA_DIR", original);
        }
    }

    #[test]
    fn test_get_first_commit_time() {
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
            email: "test@example.com".to_string(),
            signature: None,
        };

        // Initialise clinical service
        let service = ClinicalService;

        // Call initialise to create a record
        let clinical_uuid = service
            .initialise(author)
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
            email: "test@example.com".to_string(),
            signature: Some(private_key_pem.to_string()),
        };

        // Initialise clinical record
        let result = service.initialise(author);
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
}
