//! # VPR Core
//!
//! Core business logic for the VPR patient record system.
//!
//! This crate contains pure data operations and file/folder management:
//! - Patient creation and listing with sharded JSON storage
//! - File system operations under the configured patient data directory
//! - Git-like versioning
//!
//! **No API concerns**: Authentication, HTTP/gRPC servers, or service interfaces belong in `api-grpc`, `api-rest`, or `api-shared`.

pub mod clinical;
pub mod config;
pub mod constants;
pub mod demographics;
pub(crate) mod git;
pub mod repo;
pub(crate) mod uuid;
pub mod validation;

// Use the shared api-shared crate for generated protobuf types.
pub use api_shared::pb;

// Re-export commonly used constants
pub use constants::DEFAULT_PATIENT_DATA_DIR;

pub use config::CoreConfig;

use base64::{engine::general_purpose, Engine as _};
use serde::Deserialize;
#[allow(clippy::single_component_path_imports)]
use serde_yaml;
use std::fs;
use std::path::Path;
use x509_parser::prelude::*;

#[derive(Clone, Debug)]
pub struct Author {
    pub name: String,
    pub role: String,
    pub email: String,
    pub registrations: Vec<AuthorRegistration>,
    pub signature: Option<String>,
    /// Optional X.509 certificate for the author.
    ///
    /// This is treated as opaque bytes and may be PEM or DER.
    /// When present and the commit is signed, it must correspond to the signing key.
    pub certificate: Option<Vec<u8>>,
}

/// Material embedded in the Git commit object to enable offline verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddedCommitSignature {
    /// Raw 64-byte ECDSA P-256 signature (`r || s`).
    pub signature: Vec<u8>,
    /// SEC1-encoded public key bytes.
    pub public_key: Vec<u8>,
    /// Optional X.509 certificate bytes (PEM or DER).
    pub certificate: Option<Vec<u8>>,
}

#[derive(Deserialize)]
struct VprCommitSignaturePayloadV1 {
    signature: String,
    public_key: String,
    #[serde(default)]
    certificate: Option<String>,
}

fn extract_cert_public_key_sec1(cert_bytes: &[u8]) -> PatientResult<Vec<u8>> {
    let cert_der: Vec<u8> = if cert_bytes
        .windows("-----BEGIN CERTIFICATE-----".len())
        .any(|w| w == b"-----BEGIN CERTIFICATE-----")
    {
        let (_, pem) = x509_parser::pem::parse_x509_pem(cert_bytes)
            .map_err(|e| PatientError::EcdsaPublicKeyParse(Box::new(e)))?;
        pem.contents.to_vec()
    } else {
        cert_bytes.to_vec()
    };

    let (_, cert) = X509Certificate::from_der(cert_der.as_slice())
        .map_err(|e| PatientError::EcdsaPublicKeyParse(Box::new(e)))?;
    let spk = cert.public_key();
    Ok(spk.subject_public_key.data.to_vec())
}

/// Extract the embedded signature material from a commit.
///
/// VPR stores a base64-encoded JSON container in the commit's `gpgsig` header that includes:
/// - `signature` (base64 raw 64-byte `r||s`)
/// - `public_key` (base64 SEC1 public key bytes)
/// - optional `certificate` (base64 of PEM or DER bytes)
///
/// If a certificate is present, this validates that it corresponds to the embedded public key.
pub fn extract_embedded_commit_signature(
    commit: &git2::Commit<'_>,
) -> PatientResult<EmbeddedCommitSignature> {
    let sig_field = commit
        .header_field_bytes("gpgsig")
        .map_err(|_| PatientError::InvalidCommitSignaturePayload)?;
    if sig_field.is_empty() {
        return Err(PatientError::InvalidCommitSignaturePayload);
    }

    let sig_field_str = std::str::from_utf8(sig_field.as_ref())
        .map_err(|_| PatientError::InvalidCommitSignaturePayload)?;
    let sig_b64: String = sig_field_str.lines().map(|l| l.trim()).collect();

    let payload_bytes = general_purpose::STANDARD
        .decode(sig_b64)
        .map_err(|_| PatientError::InvalidCommitSignaturePayload)?;
    let payload: VprCommitSignaturePayloadV1 = serde_json::from_slice(&payload_bytes)
        .map_err(|_| PatientError::InvalidCommitSignaturePayload)?;

    let signature = general_purpose::STANDARD
        .decode(payload.signature)
        .map_err(|_| PatientError::InvalidCommitSignaturePayload)?;
    let public_key = general_purpose::STANDARD
        .decode(payload.public_key)
        .map_err(|_| PatientError::InvalidCommitSignaturePayload)?;
    let certificate = match payload.certificate {
        Some(cert_b64) => Some(
            general_purpose::STANDARD
                .decode(cert_b64)
                .map_err(|_| PatientError::InvalidCommitSignaturePayload)?,
        ),
        None => None,
    };

    if let Some(cert_bytes) = certificate.as_deref() {
        let cert_public_key = extract_cert_public_key_sec1(cert_bytes)?;
        if cert_public_key != public_key {
            return Err(PatientError::AuthorCertificatePublicKeyMismatch);
        }
    }

    Ok(EmbeddedCommitSignature {
        signature,
        public_key,
        certificate,
    })
}

/// A declared professional registration for an author.
///
/// This is rendered in commit trailers as:
///
/// `Author-Registration: <authority> <number>`
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct AuthorRegistration {
    pub authority: String,
    pub number: String,
}

impl AuthorRegistration {
    pub fn new(authority: impl Into<String>, number: impl Into<String>) -> PatientResult<Self> {
        let authority = authority.into().trim().to_string();
        let number = number.into().trim().to_string();

        if authority.is_empty()
            || number.is_empty()
            || authority.contains(['\n', '\r'])
            || number.contains(['\n', '\r'])
            || authority.chars().any(char::is_whitespace)
            || number.chars().any(char::is_whitespace)
        {
            return Err(PatientError::InvalidAuthorRegistration);
        }

        Ok(Self { authority, number })
    }
}

impl Author {
    /// Validate that this author contains the mandatory commit author metadata.
    ///
    /// This validation is intended to run before commit creation/signing.
    pub fn validate_commit_author(&self) -> PatientResult<()> {
        if self.name.trim().is_empty() {
            return Err(PatientError::MissingAuthorName);
        }
        if self.role.trim().is_empty() {
            return Err(PatientError::MissingAuthorRole);
        }

        for reg in &self.registrations {
            AuthorRegistration::new(reg.authority.clone(), reg.number.clone())?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod author_tests {
    use super::*;

    fn base_author() -> Author {
        Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        }
    }

    #[test]
    fn validate_commit_author_rejects_missing_name() {
        let mut author = base_author();
        author.name = "\t\n".to_string();

        let err = author
            .validate_commit_author()
            .expect_err("expected validation failure");
        assert!(matches!(err, PatientError::MissingAuthorName));
    }

    #[test]
    fn validate_commit_author_rejects_missing_role() {
        let mut author = base_author();
        author.role = " ".to_string();

        let err = author
            .validate_commit_author()
            .expect_err("expected validation failure");
        assert!(matches!(err, PatientError::MissingAuthorRole));
    }

    #[test]
    fn validate_commit_author_rejects_invalid_registration() {
        let mut author = base_author();
        author.registrations = vec![AuthorRegistration {
            authority: "G MC".to_string(),
            number: "12345".to_string(),
        }];

        let err = author
            .validate_commit_author()
            .expect_err("expected validation failure");
        assert!(matches!(err, PatientError::InvalidAuthorRegistration));
    }

    #[test]
    fn validate_commit_author_accepts_valid_author() {
        let mut author = base_author();
        author.registrations = vec![AuthorRegistration {
            authority: "GMC".to_string(),
            number: "12345".to_string(),
        }];

        author
            .validate_commit_author()
            .expect("expected validation to succeed");
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PatientError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("failed to create storage directory: {0}")]
    StorageDirCreation(std::io::Error),
    #[error("failed to create patient directory: {0}")]
    PatientDirCreation(std::io::Error),
    #[error(
        "initialise failed and cleanup also failed (path: {path}): init={init_error}; cleanup={cleanup_error}",
        path = path.display()
    )]
    CleanupAfterInitialiseFailed {
        path: std::path::PathBuf,
        #[source]
        init_error: Box<PatientError>,
        cleanup_error: std::io::Error,
    },
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
    #[error("failed to deserialize YAML: {0}")]
    YamlDeserialization(serde_yaml::Error),

    #[error("openEHR error: {0}")]
    Openehr(#[from] openehr::OpenEhrError),
    #[error("failed to initialise git repository: {0}")]
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
    #[error("failed to parse PEM: {0}")]
    PemParse(::pem::PemError),
    #[error("failed to parse ECDSA private key: {0}")]
    EcdsaPrivateKeyParse(Box<dyn std::error::Error + Send + Sync>),
    #[error("failed to parse ECDSA public key/certificate: {0}")]
    EcdsaPublicKeyParse(Box<dyn std::error::Error + Send + Sync>),
    #[error("author certificate public key does not match signing key")]
    AuthorCertificatePublicKeyMismatch,
    #[error("invalid embedded commit signature payload")]
    InvalidCommitSignaturePayload,
    #[error("failed to sign: {0}")]
    EcdsaSign(Box<dyn std::error::Error + Send + Sync>),
    #[error("failed to create commit buffer: {0}")]
    GitCommitBuffer(git2::Error),
    #[error("failed to create signed commit: {0}")]
    GitCommitSigned(git2::Error),
    #[error("failed to convert commit buffer to string: {0}")]
    CommitBufferToString(std::string::FromUtf8Error),
    #[error("failed to open git repository: {0}")]
    GitOpen(git2::Error),
    #[error("failed to create/update git reference: {0}")]
    GitReference(git2::Error),
    #[error("failed to get git head: {0}")]
    GitHead(git2::Error),
    #[error("failed to set git head: {0}")]
    GitSetHead(git2::Error),
    #[error("failed to peel git commit: {0}")]
    GitPeel(git2::Error),
    #[error("invalid timestamp")]
    InvalidTimestamp,

    #[error("missing Author-Name")]
    MissingAuthorName,
    #[error("missing Author-Role")]
    MissingAuthorRole,
    #[error("invalid Author-Registration")]
    InvalidAuthorRegistration,
    #[error("author trailer keys are reserved")]
    ReservedAuthorTrailerKey,

    #[error("invalid Care-Location")]
    InvalidCareLocation,
    #[error("missing Care-Location")]
    MissingCareLocation,
    #[error("Care-Location trailer key is reserved")]
    ReservedCareLocationTrailerKey,
}

pub type PatientResult<T> = std::result::Result<T, PatientError>;

/// Represents a complete patient record with both demographics and clinical components.
#[derive(Debug)]
pub struct FullRecord {
    /// The UUID of the demographics record.
    pub demographics_uuid: String,
    /// The UUID of the clinical record.
    pub clinical_uuid: String,
}

/// Pure patient data operations - no API concerns
#[derive(Clone)]
pub struct PatientService {
    cfg: std::sync::Arc<CoreConfig>,
}

impl PatientService {
    /// Creates a new instance of PatientService.
    ///
    /// # Returns
    /// A new `PatientService` instance ready to handle patient operations.
    pub fn new(cfg: std::sync::Arc<CoreConfig>) -> Self {
        Self { cfg }
    }

    /// Initialises a complete patient record with demographics and clinical components.
    ///
    /// This function creates both a demographics repository and a clinical repository,
    /// links them together, and populates the demographics with the provided patient information.
    ///
    /// # Arguments
    ///
    /// * `author` - The author information for Git commits.
    /// * `given_names` - A vector of the patient's given names.
    /// * `last_name` - The patient's family/last name.
    /// * `birth_date` - The patient's date of birth as a string (e.g., "YYYY-MM-DD").
    /// * `namespace` - Optional namespace for the clinical-demographics link.
    ///
    /// # Returns
    ///
    /// Returns a `FullRecord` containing both UUIDs on success.
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if:
    /// - demographics initialisation or update fails,
    /// - clinical initialisation fails,
    /// - linking clinical to demographics fails.
    pub fn initialise_full_record(
        &self,
        author: Author,
        care_location: String,
        given_names: Vec<String>,
        last_name: String,
        birth_date: String,
        namespace: Option<String>,
    ) -> PatientResult<FullRecord> {
        let demographics_service = crate::demographics::DemographicsService::new(self.cfg.clone());
        // Initialise demographics
        let demographics_uuid =
            demographics_service.initialise(author.clone(), care_location.clone())?;

        // Update demographics with patient information
        demographics_service.update(&demographics_uuid, given_names, &last_name, &birth_date)?;

        // Initialise clinical
        let clinical_service = crate::clinical::ClinicalService::new(self.cfg.clone());
        let clinical_uuid = clinical_service.initialise(author.clone(), care_location.clone())?;

        // Link clinical to demographics
        clinical_service.link_to_demographics(
            &author,
            care_location,
            &clinical_uuid.simple().to_string(),
            &demographics_uuid,
            namespace,
        )?;

        Ok(FullRecord {
            demographics_uuid,
            clinical_uuid: clinical_uuid.simple().to_string(),
        })
    }
}

/// YAML file management utilities
///
/// These functions provide safe operations for creating and updating YAML files
/// using serde_yaml::Value for maximum flexibility in merging and modifying data.
/// Updates a YAML file by merging new data with existing content (or creating if not exists).
///
/// This function reads the current YAML file (if it exists), merges the new data into it,
/// and writes back the result. Merging is performed as follows:
/// - If file doesn't exist: uses new_data as-is
/// - If both current and new_data are Mappings: merges new_data fields into current
/// - Otherwise: replaces current with new_data
///
/// # Arguments
/// * `path` - Path to the YAML file
/// * `new_data` - The new data to merge into the existing file
pub fn yaml_write<T: serde::Serialize>(path: &Path, new_data: &T) -> Result<(), PatientError> {
    let new_value = serde_yaml::to_value(new_data).map_err(PatientError::YamlSerialization)?;
    let current = if path.exists() {
        let yaml_str = fs::read_to_string(path).map_err(PatientError::FileRead)?;
        serde_yaml::from_str(&yaml_str).map_err(PatientError::YamlDeserialization)?
    } else {
        serde_yaml::Value::Null
    };

    let merged_value = merge_yaml_values(current, new_value);
    let yaml = serde_yaml::to_string(&merged_value).map_err(PatientError::YamlSerialization)?;
    fs::write(path, yaml).map_err(PatientError::FileWrite)
}

/// Merges two YAML values according to the yaml_write rules
fn merge_yaml_values(current: serde_yaml::Value, new_data: serde_yaml::Value) -> serde_yaml::Value {
    match (current, new_data) {
        (serde_yaml::Value::Null, new) => new,
        (serde_yaml::Value::Mapping(mut current_map), serde_yaml::Value::Mapping(new_map)) => {
            // Merge new_map into current_map
            for (key, value) in new_map {
                current_map.insert(key, value);
            }
            serde_yaml::Value::Mapping(current_map)
        }
        (_, new) => new, // Replace current with new for non-mapping cases
    }
}

/// Recursively copies a directory and its contents to a destination.
///
/// This function creates the destination directory if it doesn't exist and
/// copies all files and subdirectories from the source to the destination.
///
/// # Arguments
/// * `src` - Source directory path
/// * `dst` - Destination directory path
///
/// # Errors
/// Returns an `std::io::Error` if:
/// - creating the destination directory fails,
/// - reading source directory entries fails,
/// - inspecting entry types fails,
/// - copying a file fails.
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// Adds all files in a directory to a Git index recursively.
///
/// This function traverses the directory tree and adds all files to the Git index,
/// creating a tree that can be committed. It skips .git directories.
///
/// # Arguments
/// * `index` - Mutable reference to the Git index
/// * `dir` - Directory path to add to the index
///
/// # Errors
/// Returns a `git2::Error` if:
/// - traversing the directory tree fails,
/// - inspecting file types fails,
/// - adding a path to the Git index fails.
pub fn add_directory_to_index(index: &mut git2::Index, dir: &Path) -> Result<(), git2::Error> {
    fn add_recursive(
        index: &mut git2::Index,
        dir: &Path,
        prefix: &Path,
    ) -> Result<(), git2::Error> {
        for entry in std::fs::read_dir(dir).map_err(|e| git2::Error::from_str(&e.to_string()))? {
            let entry = entry.map_err(|e| git2::Error::from_str(&e.to_string()))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|e| git2::Error::from_str(&e.to_string()))?;

            // Skip .git directories
            if path.ends_with(".git") {
                continue;
            }

            if file_type.is_file() {
                let relative_path = path.strip_prefix(prefix).unwrap();
                index.add_path(relative_path)?;
            } else if file_type.is_dir() {
                add_recursive(index, &path, prefix)?;
            }
        }
        Ok(())
    }

    add_recursive(index, dir, dir)
}
