//! Versioned file operations with Git-based version control for VPR.
//!
//! VPR stores patient data as files on disk and versions each patient directory using a
//! local Git repository (`git2`/libgit2). This module provides high-level services for
//! managing versioned files, ensuring:
//!
//! - **Atomic Multi-file Operations**: Write multiple files and commit them in a single
//!   transaction with automatic rollback on failure
//! - **Consistent Commit Creation**: Structured commit messages with controlled vocabulary
//!   across all services (clinical, demographics, coordination)
//! - **Cryptographic Signing**: ECDSA P-256 signatures with X.509 certificate validation
//! - **Immutable Audit Trail**: Nothing is ever deleted; all changes are preserved in
//!   version control history for patient safety and legal compliance
//!
//! ## Purpose and Scope
//!
//! This module is the core of VPR's version control system. It centralises Git operations
//! to ensure consistency and safety when modifying patient records. The [`VersionedFileService`]
//! provides high-level operations that handle directory creation, file writing, Git commits,
//! and automatic rollback on errors.
//!
//! The module supports both signed (with X.509 certificates) and unsigned commits, with
//! signature verification available for auditing purposes.
//!
//! ## Architecture
//!
//! The module provides four main components:
//!
//! - **File Operations**: [`FileToWrite`] struct for describing atomic file write operations
//! - **Repository Management**: [`VersionedFileService`] for high-level Git operations
//! - **Commit Messages**: [`VprCommitMessage`] with structured domains and actions
//! - **Cryptographic Signing**: ECDSA P-256 signature creation and verification
//!
//! ## Branch Policy
//!
//! VPR standardises on `refs/heads/main` for all patient repositories.
//!
//! libgit2's `commit_signed` creates a commit object but **does not update refs** (no branch
//! movement and no `HEAD` update). For signed commits, this module explicitly updates
//! `refs/heads/main` and points `HEAD` to it to maintain proper branch state.
//!
//! ## Signature Format
//!
//! When `Author.signature` is present, VPR signs commits using ECDSA P-256.
//!
//! - Signed payload: the *unsigned commit buffer* produced by `Repository::commit_create_buffer`
//! - Signature bytes: raw 64 bytes (`r || s`, not DER)
//! - Stored form: base64 of a deterministic JSON container passed to `commit_signed` and
//!   written into the commit header field `gpgsig`
//!
//! The container embeds:
//! - `signature`: base64 of raw 64-byte `r||s`
//! - `public_key`: base64 of SEC1-encoded public key bytes
//! - `certificate` (optional): base64 of the certificate bytes (PEM or DER)
//!
//! ## Safety and Immutability
//!
//! VPR maintains an immutable audit trail where nothing is ever truly deleted. The
//! [`VprCommitAction`] enum documents the four allowed operations (Create, Update,
//! Superseded, Redact), all of which preserve historical data in version control.
//! This design ensures patient safety, legal compliance, and complete accountability
//! for all modifications to patient records.
//!
//! The verifier in clinical code (`ClinicalService::verify_commit_signature`) expects this
//! exact scheme.

use crate::author::Author;
use crate::error::{PatientError, PatientResult};
use crate::types::NonEmptyText;
use crate::ShardableUuid;
use base64::{engine::general_purpose, Engine as _};
use p256::ecdsa::signature::{Signer, Verifier};
use p256::ecdsa::{Signature, SigningKey, VerifyingKey};
use p256::pkcs8::{DecodePrivateKey, DecodePublicKey};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use x509_parser::prelude::*;

#[cfg(test)]
use std::collections::HashSet;
#[cfg(test)]
use std::sync::{LazyLock, Mutex};

const MAIN_REF: &str = "refs/heads/main";

/// Deterministic container for VPR commit signatures.
///
/// This struct holds the cryptographic components of a VPR commit signature.
/// It is serialized to JSON with a stable field order (struct order), then base64-encoded
/// and stored as the `gpgsig` header value via `git2::Repository::commit_signed`.
///
/// The container ensures that all signature metadata is embedded directly in the Git commit,
/// making signatures self-contained and verifiable without external dependencies.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct VprCommitSignaturePayloadV1 {
    /// Base64 of raw 64-byte ECDSA P-256 signature (`r || s`).
    signature: String,
    /// Base64 of SEC1-encoded public key bytes.
    public_key: String,
    /// Base64 of X.509 certificate bytes (PEM or DER), if provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    certificate: Option<String>,
}

/// Extract the SEC1-encoded public key bytes from an X.509 certificate.
///
/// Accepts both PEM and DER certificate formats. The certificate is parsed to extract
/// the public key, which is then encoded in SEC1 format for use with ECDSA verification.
///
/// This function is used during commit signing to validate that the author's certificate
/// matches their signing key.
///
/// # Arguments
///
/// * `cert_bytes` - Raw certificate bytes (PEM or DER format)
///
/// # Returns
///
/// SEC1-encoded public key bytes on success.
///
/// # Errors
///
/// Returns `PatientError::EcdsaPublicKeyParse` if the certificate cannot be parsed
/// or the public key cannot be extracted.
fn extract_cert_public_key_sec1(cert_bytes: &[u8]) -> PatientResult<Vec<u8>> {
    // Accept PEM or DER; treat as opaque bytes for storage but parse to validate key match.
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

/// Clinical domain categories for commit messages.
///
/// These represent different types of clinical data being modified.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ClinicalDomain {
    Record,
    Observation,
    Diagnosis,
    Treatment,
    Administration,
    Correction,
    Metadata,
}

impl ClinicalDomain {
    #[allow(dead_code)]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Observation => "observation",
            Self::Diagnosis => "diagnosis",
            Self::Treatment => "treatment",
            Self::Administration => "administration",
            Self::Correction => "correction",
            Self::Metadata => "metadata",
        }
    }
}

/// Coordination domain categories for commit messages.
///
/// These represent different types of care coordination activities.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CoordinationDomain {
    Record,
    Messaging,
}

impl CoordinationDomain {
    #[allow(dead_code)]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Messaging => "messaging",
        }
    }
}

/// Demographics domain categories for commit messages.
///
/// These represent different types of demographic data being modified.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DemographicsDomain {
    Record,
}

impl DemographicsDomain {
    #[allow(dead_code)]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Record => "record",
        }
    }
}

/// Controlled vocabulary for VPR commit message domains.
///
/// Hierarchical structure organizing commits by repository type (Clinical, Coordination, Demographics)
/// and specific domain within that repository.
///
/// Safety/intent: Do not include patient identifiers or raw clinical data in commit messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum VprCommitDomain {
    Clinical(ClinicalDomain),
    Coordination(CoordinationDomain),
    Demographics(DemographicsDomain),
}

impl VprCommitDomain {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Clinical(subdomain) => subdomain.as_str(),
            Self::Coordination(subdomain) => subdomain.as_str(),
            Self::Demographics(subdomain) => subdomain.as_str(),
        }
    }
}

impl fmt::Display for VprCommitDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for VprCommitDomain {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for VprCommitDomain {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "record" => Ok(Self::Clinical(ClinicalDomain::Record)),
            "observation" => Ok(Self::Clinical(ClinicalDomain::Observation)),
            "diagnosis" => Ok(Self::Clinical(ClinicalDomain::Diagnosis)),
            "treatment" => Ok(Self::Clinical(ClinicalDomain::Treatment)),
            "administration" => Ok(Self::Clinical(ClinicalDomain::Administration)),
            "correction" => Ok(Self::Clinical(ClinicalDomain::Correction)),
            "metadata" => Ok(Self::Clinical(ClinicalDomain::Metadata)),
            "messaging" => Ok(Self::Coordination(CoordinationDomain::Messaging)),
            _ => Err(serde::de::Error::unknown_variant(
                &s,
                &[
                    "record",
                    "observation",
                    "diagnosis",
                    "treatment",
                    "administration",
                    "correction",
                    "metadata",
                    "messaging",
                ],
            )),
        }
    }
}

/// Controlled vocabulary for VPR commit message actions.
///
/// These define the specific operation being performed on patient data.
/// Actions are designed to be machine-readable and support audit trails.
///
/// # Immutability Philosophy
///
/// VPR maintains an **immutable audit trail** - nothing is ever truly deleted from the
/// version control history. This ensures complete auditability and supports patient safety
/// by preserving all changes made to a record.
///
/// # Commit Actions
///
/// - **`Create`**: Used when adding new content to an existing record (e.g., creating a new
///   letter, adding a new observation, initializing a new patient record). This is the
///   most common action for new data entry.
///
/// - **`Update`**: Used when modifying existing content (e.g., correcting a typo in a letter,
///   updating demographics, linking records). The previous version remains in Git history.
///
/// - **`Superseded`**: Used when newer information makes previous content obsolete
///   (e.g., a revised diagnosis, an updated care plan). The superseded content remains
///   in history but is marked as no longer current. This is distinct from `Update` as it
///   represents a clinical decision that previous information should be replaced rather
///   than corrected.
///
/// - **`Redact`**: Used when data was entered into the wrong patient's repository
///   by mistake (can occur in clinical, demographics, or coordination repositories).
///   The data is removed from the current view, encrypted, and stored in the
///   Redaction Retention Repository with a tombstone/pointer remaining in the original
///   repository's Git history. This maintains audit trail integrity while protecting
///   patient privacy. **This is the only action that removes data from active view**,
///   but even redacted data is preserved in secure storage for audit purposes.
///
/// # What VPR Never Does
///
/// VPR **never deletes data** from the version control history. Even redacted data is
/// moved to secure storage rather than destroyed. This immutability is fundamental to:
///
/// - Patient safety: all changes are traceable
/// - Legal compliance: complete audit trail preservation
/// - Clinical governance: accountability for all modifications
/// - Research and quality improvement: historical data remains available for authorized use
///
/// # Safety/Intent
///
/// Do not include patient identifiers or raw clinical data in commit messages.
/// Use structured trailers for metadata only.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum VprCommitAction {
    Create,
    Update,
    Superseded,
    Redact,
}

impl VprCommitAction {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Superseded => "superseded",
            Self::Redact => "redact",
        }
    }
}

impl fmt::Display for VprCommitAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single commit trailer line in standard Git trailer format.
///
/// Renders as `Key: Value`. Trailers provide additional structured metadata
/// beyond the main commit subject line. They follow Git's standard trailer
/// conventions and are sorted deterministically in rendered output.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct VprCommitTrailer {
    key: String,
    value: String,
}

impl VprCommitTrailer {
    /// Create a new commit trailer with validation.
    ///
    /// # Arguments
    ///
    /// * `key` - Trailer key (cannot contain ':', newlines, or be empty)
    /// * `value` - Trailer value (cannot contain newlines or be empty)
    ///
    /// # Errors
    ///
    /// Returns `PatientError::InvalidInput` if validation fails.
    #[allow(dead_code)]
    pub(crate) fn new(key: impl Into<String>, value: impl Into<String>) -> PatientResult<Self> {
        let key = key.into().trim().to_string();
        let value = value.into().trim().to_string();

        if key.is_empty()
            || key.contains(['\n', '\r'])
            || key.contains(':')
            || value.is_empty()
            || value.contains(['\n', '\r'])
        {
            return Err(PatientError::InvalidInput(
                "commit trailer key/value must be non-empty and single-line (key cannot contain ':')".into()
            ));
        }

        Ok(Self { key, value })
    }

    /// Get the trailer key.
    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    /// Get the trailer value.
    pub(crate) fn value(&self) -> &str {
        &self.value
    }
}

/// A structured, predictable VPR commit message.
///
/// Rendering rules:
///
/// - Subject line: `<domain>:<action>: <summary>`
/// - Trailers (optional): standard Git trailer lines `Key: Value`
/// - If there are trailers, a single blank line separates subject from trailers.
/// - No free-form prose paragraphs.
///
/// Safety/intent: Commit messages are labels and indexes; do not include patient identifiers or
/// raw clinical data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VprCommitMessage {
    domain: VprCommitDomain,
    action: VprCommitAction,
    summary: NonEmptyText,
    care_location: NonEmptyText,
    trailers: Vec<VprCommitTrailer>,
}

impl VprCommitMessage {
    /// Create a new commit message with required fields.
    ///
    /// # Arguments
    ///
    /// * `domain` - The category of change (e.g., Record, Obs, Dx)
    /// * `action` - The specific operation (e.g., Init, Add, Update)
    /// * `summary` - Brief description of the change (single line, no newlines)
    /// * `care_location` - Where the change occurred (e.g., "St Elsewhere Hospital")
    ///
    /// # Errors
    ///
    /// Returns `PatientError::InvalidInput` if summary contains newlines or is empty.
    /// Returns `PatientError::MissingCareLocation` if care_location is empty.
    /// Returns `PatientError::InvalidCareLocation` if care_location contains newlines.
    #[allow(dead_code)]
    pub(crate) fn new(
        domain: VprCommitDomain,
        action: VprCommitAction,
        summary: impl AsRef<str>,
        care_location: impl AsRef<str>,
    ) -> PatientResult<Self> {
        let summary_str = summary.as_ref().trim();
        if summary_str.contains(['\n', '\r']) {
            return Err(PatientError::InvalidInput(
                "commit summary must be single-line".into(),
            ));
        }
        let summary = NonEmptyText::new(summary_str)
            .map_err(|_| PatientError::InvalidInput("commit summary must be non-empty".into()))?;

        let care_location_str = care_location.as_ref().trim();
        if care_location_str.contains(['\n', '\r']) {
            return Err(PatientError::InvalidCareLocation);
        }
        let care_location =
            NonEmptyText::new(care_location_str).map_err(|_| PatientError::MissingCareLocation)?;

        Ok(Self {
            domain,
            action,
            summary,
            care_location,
            trailers: Vec::new(),
        })
    }

    /// Add a trailer to the commit message.
    ///
    /// Trailers provide additional structured metadata. Certain trailer keys
    /// are reserved (Author-* and Care-Location) and cannot be set manually.
    ///
    /// # Arguments
    ///
    /// * `key` - Trailer key (cannot contain ':', newlines, or be reserved)
    /// * `value` - Trailer value (cannot contain newlines or be empty)
    ///
    /// # Errors
    ///
    /// Returns `PatientError::ReservedAuthorTrailerKey` for Author-* keys.
    /// Returns `PatientError::ReservedCareLocationTrailerKey` for Care-Location key.
    /// Returns `PatientError::InvalidInput` for invalid key/value format.
    #[allow(dead_code)]
    pub(crate) fn with_trailer(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> PatientResult<Self> {
        let key_str = key.into();
        if key_str.trim_start().starts_with("Author-") {
            return Err(PatientError::ReservedAuthorTrailerKey);
        }
        if key_str.trim() == "Care-Location" {
            return Err(PatientError::ReservedCareLocationTrailerKey);
        }
        self.trailers
            .push(VprCommitTrailer::new(key_str, value.into())?);
        Ok(self)
    }

    /// Get the commit domain.
    #[allow(dead_code)]
    pub(crate) fn domain(&self) -> VprCommitDomain {
        self.domain
    }

    /// Get the commit action.
    #[allow(dead_code)]
    pub(crate) fn action(&self) -> VprCommitAction {
        self.action
    }

    /// Get the commit summary.
    #[allow(dead_code)]
    pub(crate) fn summary(&self) -> &str {
        self.summary.as_str()
    }

    /// Get the commit trailers.
    #[allow(dead_code)]
    pub(crate) fn trailers(&self) -> &[VprCommitTrailer] {
        &self.trailers
    }

    /// Render the commit message without author information.
    ///
    /// Produces a standard Git commit message format with subject line and trailers.
    /// Does not include Author-* trailers (use `render_with_author` for that).
    ///
    /// # Returns
    ///
    /// A properly formatted Git commit message string.
    ///
    /// # Errors
    ///
    /// Returns `PatientError` if the message contains invalid data (should not happen
    /// if constructed via `new()` and `with_trailer()`).
    #[allow(dead_code)]
    pub(crate) fn render(&self) -> PatientResult<String> {
        // With NonEmptyText, validation is enforced at construction time
        let mut rendered = format!("{}:{}: {}", self.domain, self.action, self.summary.as_str());

        // Sort non-reserved trailers deterministically.
        let mut other = self.trailers.clone();
        other.sort_by(|a, b| {
            let a_key = (a.key().trim(), a.value().trim());
            let b_key = (b.key().trim(), b.value().trim());
            a_key.cmp(&b_key)
        });

        rendered.push_str("\n\n");
        rendered.push_str("Care-Location: ");
        rendered.push_str(self.care_location.as_str());

        for trailer in other {
            if trailer.key().contains(['\n', '\r'])
                || trailer.key().trim().is_empty()
                || trailer.key().contains(':')
                || trailer.value().contains(['\n', '\r'])
            {
                return Err(PatientError::InvalidInput(
                    "commit trailer key/value must be non-empty and single-line (key cannot contain ':')".into()
                ));
            }

            // Author trailers are rendered via `render_with_author` only.
            if trailer.key().trim_start().starts_with("Author-") {
                return Err(PatientError::ReservedAuthorTrailerKey);
            }

            // Care-Location is rendered via `with_care_location` only.
            if trailer.key().trim() == "Care-Location" {
                return Err(PatientError::ReservedCareLocationTrailerKey);
            }

            rendered.push('\n');
            rendered.push_str(trailer.key().trim());
            rendered.push_str(": ");
            rendered.push_str(trailer.value().trim());
        }

        Ok(rendered)
    }

    /// Render a commit message including mandatory Author trailers.
    ///
    /// The Author trailers are rendered deterministically in the order:
    ///
    /// - `Author-Name`
    /// - `Author-Role`
    /// - `Author-Registration` (0..N; sorted)
    ///
    /// This is the method used by `VersionedFileService` for creating commits.
    ///
    /// # Arguments
    ///
    /// * `author` - Author information including name, role, and registrations
    ///
    /// # Returns
    ///
    /// A complete Git commit message with author metadata.
    ///
    /// # Errors
    ///
    /// Returns `PatientError` from author validation or message rendering.
    pub(crate) fn render_with_author(&self, author: &Author) -> PatientResult<String> {
        author.validate_commit_author()?;

        // Author trailers are reserved and must only be emitted from the structured metadata.
        if self
            .trailers
            .iter()
            .any(|t| t.key().trim_start().starts_with("Author-"))
        {
            return Err(PatientError::ReservedAuthorTrailerKey);
        }

        // Care-Location is reserved and must only be emitted from the structured metadata.
        if self
            .trailers
            .iter()
            .any(|t| t.key().trim() == "Care-Location")
        {
            return Err(PatientError::ReservedCareLocationTrailerKey);
        }

        // With NonEmptyText, validation is enforced at construction time
        let mut rendered = format!("{}:{}: {}", self.domain, self.action, self.summary.as_str());

        // Sort registrations deterministically, but do not require selecting a primary authority.
        let mut regs = author.registrations.clone();
        regs.sort_by(|a, b| {
            let a_key = (a.authority.as_str(), a.number.as_str());
            let b_key = (b.authority.as_str(), b.number.as_str());
            a_key.cmp(&b_key)
        });

        // Sort non-author trailers deterministically.
        let mut other = self.trailers.clone();
        other.sort_by(|a, b| {
            let a_key = (a.key().trim(), a.value().trim());
            let b_key = (b.key().trim(), b.value().trim());
            a_key.cmp(&b_key)
        });

        rendered.push_str("\n\n");
        rendered.push_str("Author-Name: ");
        rendered.push_str(author.name.trim());
        rendered.push('\n');
        rendered.push_str("Author-Role: ");
        rendered.push_str(author.role.trim());

        for reg in regs {
            rendered.push('\n');
            rendered.push_str("Author-Registration: ");
            rendered.push_str(reg.authority.trim());
            rendered.push(' ');
            rendered.push_str(reg.number.trim());
        }

        rendered.push('\n');
        rendered.push_str("Care-Location: ");
        rendered.push_str(self.care_location.as_str());

        for trailer in other {
            rendered.push('\n');
            rendered.push_str(trailer.key().trim());
            rendered.push_str(": ");
            rendered.push_str(trailer.value().trim());
        }

        Ok(rendered)
    }
}

/// Service for common Git operations on a repository rooted at `workdir`.
///
/// This bundles the repository handle and its workdir to make workflows like “initialise repo
/// then commit files” ergonomic at call sites.
/// Represents a file to be written and committed.
///
/// Used with [`VersionedFileService::write_and_commit_files`] to write multiple files
/// in a single atomic commit operation.
#[derive(Debug, Clone)]
pub struct FileToWrite<'a> {
    /// The relative path to the file within the repository directory.
    pub relative_path: &'a Path,
    /// The new content to write to the file.
    pub content: &'a str,
    /// The previous file content for rollback. `None` if this is a new file.
    pub old_content: Option<&'a str>,
}

/// Service for managing versioned files with Git version control.
///
/// `VersionedFileService` provides high-level operations for working with Git repositories
/// in VPR's patient record system. It handles atomic file write and commit operations with
/// automatic rollback on failure, ensuring data consistency and integrity.
///
/// The service supports both signed commits (using ECDSA P-256 with X.509 certificates)
/// and unsigned commits. All commits use structured [`VprCommitMessage`] format for
/// consistency and auditability.
///
/// # Core Capabilities
///
/// - **Atomic Operations**: Write multiple files and commit in a single transaction
/// - **Automatic Rollback**: Restore previous state if any operation fails
/// - **Directory Creation**: Automatically create parent directories as needed
/// - **Signed Commits**: Optional cryptographic signing with X.509 certificates
/// - **Signature Verification**: Verify commit signatures for audit purposes
///
/// # Usage Pattern
///
/// The typical workflow is:
/// 1. Create or open a repository with [`init`](Self::init) or [`open`](Self::open)
/// 2. Prepare file changes using [`FileToWrite`] structs
/// 3. Write and commit files with [`write_and_commit_files`](Self::write_and_commit_files)
/// 4. Optionally verify signatures with [`verify_commit_signature`](Self::verify_commit_signature)
pub struct VersionedFileService {
    repo: git2::Repository,
    workdir: PathBuf,
}

impl VersionedFileService {
    /// Create a new Git repository at the specified working directory.
    ///
    /// Initialises a new Git repository using libgit2. The repository will be created
    /// with a standard `.git` directory structure. The working directory path is captured
    /// and used for all subsequent file operations.
    ///
    /// # Arguments
    ///
    /// * `workdir` - Path where the Git repository should be initialised
    ///
    /// # Returns
    ///
    /// A `VersionedFileService` instance bound to the newly created repository.
    ///
    /// # Errors
    ///
    /// Returns `PatientError` if:
    /// - Repository initialisation fails (e.g., permissions, invalid path) - [`PatientError::GitInit`]
    /// - The repository has no working directory (bare repo) - [`PatientError::GitInit`]
    pub(crate) fn init(workdir: &Path) -> PatientResult<Self> {
        let repo = git2::Repository::init(workdir).map_err(PatientError::GitInit)?;
        // Use the actual workdir from the repository to ensure path stripping works correctly.
        let actual_workdir = repo
            .workdir()
            .ok_or_else(|| {
                PatientError::GitInit(git2::Error::from_str("repository has no working directory"))
            })?
            .to_path_buf();
        Ok(Self {
            repo,
            workdir: actual_workdir,
        })
    }

    /// Open an existing Git repository at the specified working directory.
    ///
    /// Opens an existing repository using `NO_SEARCH` flag to prevent git2 from searching
    /// parent directories for a `.git` folder. This ensures we open exactly the repository
    /// at the specified path, which is important for patient record isolation.
    ///
    /// The actual working directory path is extracted from the opened repository to handle
    /// potential symlink resolution or path canonicalisation by git2.
    ///
    /// # Arguments
    ///
    /// * `workdir` - Path to the existing Git repository's working directory
    ///
    /// # Returns
    ///
    /// A `VersionedFileService` instance bound to the opened repository.
    ///
    /// # Errors
    ///
    /// Returns `PatientError` if:
    /// - Repository does not exist at the specified path - [`PatientError::GitOpen`]
    /// - Repository cannot be opened (e.g., permissions, corruption) - [`PatientError::GitOpen`]
    /// - The repository has no working directory (bare repo) - [`PatientError::GitOpen`]
    pub(crate) fn open(workdir: &Path) -> PatientResult<Self> {
        let repo = git2::Repository::open_ext(
            workdir,
            git2::RepositoryOpenFlags::NO_SEARCH,
            std::iter::empty::<&std::ffi::OsStr>(),
        )
        .map_err(PatientError::GitOpen)?;
        // Use the actual workdir from the repository to ensure path stripping works correctly.
        // git2 may resolve symlinks or canonicalize paths differently.
        let actual_workdir = repo
            .workdir()
            .ok_or_else(|| {
                PatientError::GitOpen(git2::Error::from_str("repository has no working directory"))
            })?
            .to_path_buf();
        Ok(Self {
            repo,
            workdir: actual_workdir,
        })
    }

    /// Consume this service and return the underlying `git2::Repository`.
    ///
    /// This method transfers ownership of the Git repository handle to the caller,
    /// allowing direct access to lower-level Git operations when needed. The service
    /// is consumed and cannot be used after this call.
    ///
    /// # Returns
    ///
    /// The underlying `git2::Repository` instance.
    #[allow(dead_code)]
    pub(crate) fn into_repo(self) -> git2::Repository {
        self.repo
    }

    /// Ensure `HEAD` points at `refs/heads/main`.
    ///
    /// Sets the repository's HEAD reference to point to the main branch. For newly
    /// initialised repositories this creates an "unborn" `main` branch that will be
    /// born when the first commit is written.
    ///
    /// # Errors
    ///
    /// Returns `PatientError::GitSetHead` if the HEAD reference cannot be updated.
    fn ensure_main_head(&self) -> PatientResult<()> {
        self.repo
            .set_head(MAIN_REF)
            .map_err(PatientError::GitSetHead)?;
        Ok(())
    }

    /// Create a commit including only the provided file paths (relative to the repo workdir).
    ///
    /// This is useful for “surgical” updates where you don’t want to commit everything.
    ///
    /// # Path rules
    ///
    /// `relative_paths` may contain:
    ///
    /// - repo-workdir-relative paths (recommended), or
    /// - absolute paths under the repo workdir (they will be normalised to relative paths).
    ///
    /// Paths containing `..` are rejected.
    pub(crate) fn commit_paths(
        &self,
        author: &Author,
        message: &VprCommitMessage,
        relative_paths: &[PathBuf],
    ) -> PatientResult<git2::Oid> {
        let rendered = message.render_with_author(author)?;
        self.commit_paths_rendered(author, &rendered, relative_paths)
    }

    /// Writes multiple files and commits them to Git with rollback on failure.
    ///
    /// Opens an existing Git repository, creates any necessary parent directories,
    /// writes all files, and commits them in a single Git commit. All operations are
    /// wrapped in a closure to enable automatic rollback if any operation fails. On error:
    /// - Files that previously existed are restored to their previous state
    /// - New files are removed
    /// - Any directories created during this operation are removed
    ///
    /// # Arguments
    ///
    /// * `repo_path` - Path to the existing Git repository (patient directory).
    /// * `author` - The author information for the Git commit.
    /// * `msg` - The commit message structure containing domain, action, and location.
    /// * `files` - Slice of [`FileToWrite`] structs describing files to write.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if repository opening, directory creation, all file writes,
    /// and Git commit succeed.
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if:
    /// - Repository opening fails (various Git-related error variants)
    /// - Parent directory creation fails ([`PatientError::FileWrite`])
    /// - Any file write fails ([`PatientError::FileWrite`])
    /// - The Git commit fails (various Git-related error variants)
    ///
    /// On error, attempts to rollback all files and any newly created directories.
    pub(crate) fn write_and_commit_files(
        repo_path: &Path,
        author: &Author,
        msg: &VprCommitMessage,
        files: &[FileToWrite],
    ) -> PatientResult<()> {
        let repo = Self::open(repo_path)?;

        let mut created_dirs: Vec<PathBuf> = Vec::new();
        let mut written_files: Vec<(PathBuf, Option<String>)> = Vec::new();

        let result: PatientResult<()> = (|| {
            // Collect all unique parent directories needed
            let mut dirs_needed = std::collections::HashSet::new();
            for file in files {
                let full_path = repo.workdir.join(file.relative_path);
                if let Some(parent) = full_path.parent() {
                    let mut current = parent;
                    while current != repo.workdir && !current.exists() {
                        dirs_needed.insert(current.to_path_buf());
                        if let Some(parent_of_current) = current.parent() {
                            current = parent_of_current;
                        } else {
                            break;
                        }
                    }
                }
            }

            // Sort directories by depth (shallowest first) for creation
            let mut dirs_to_create: Vec<PathBuf> = dirs_needed.into_iter().collect();
            dirs_to_create.sort_by_key(|p| p.components().count());

            // Create directories
            for dir in &dirs_to_create {
                std::fs::create_dir(dir).map_err(PatientError::FileWrite)?;
                created_dirs.push(dir.clone());
            }

            // Write all files
            for file in files {
                let full_path = repo.workdir.join(file.relative_path);
                let old_content = file.old_content.map(|s| s.to_string());

                std::fs::write(&full_path, file.content).map_err(PatientError::FileWrite)?;
                written_files.push((full_path, old_content));
            }

            // Commit all files in a single commit
            let paths: Vec<PathBuf> = files
                .iter()
                .map(|f| f.relative_path.to_path_buf())
                .collect();
            repo.commit_paths(author, msg, &paths)?;

            Ok(())
        })();

        match result {
            Ok(()) => Ok(()),
            Err(write_error) => {
                // Rollback file changes (in reverse order)
                for (full_path, old_content) in written_files.iter().rev() {
                    match old_content {
                        Some(contents) => {
                            let _ = std::fs::write(full_path, contents);
                        }
                        None => {
                            let _ = std::fs::remove_file(full_path);
                        }
                    }
                }

                // Rollback newly created directories (from deepest to shallowest)
                for dir in created_dirs.iter().rev() {
                    let _ = std::fs::remove_dir(dir);
                }

                Err(write_error)
            }
        }
    }

    /// Initialise a Git repository, commit initial files, and clean up on failure.
    ///
    /// This method encapsulates the common pattern of:
    /// 1. Initialising a Git repository in a new directory
    /// 2. Writing and committing initial files
    /// 3. Automatically removing the entire directory if any error occurs
    ///
    /// This ensures atomic repository creation - either the repository is fully
    /// initialised with its initial commit, or the directory is completely removed.
    /// This is critical for maintaining consistency in patient record storage.
    ///
    /// # Arguments
    ///
    /// * `patient_dir` - Path where the Git repository should be created. This entire
    ///   directory will be removed if initialisation fails.
    /// * `author` - The author information for the initial Git commit.
    /// * `message` - The commit message structure containing domain, action, and location.
    /// * `files` - Slice of [`FileToWrite`] structs describing initial files to write.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if repository initialisation, file writes, and commit succeed.
    ///
    /// # Errors
    ///
    /// Returns a `PatientError` if:
    /// - Repository initialisation fails ([`PatientError::GitInit`])
    /// - File writes fail ([`PatientError::FileWrite`])
    /// - Git commit fails (various Git-related error variants)
    ///
    /// On error, attempts to remove the entire `patient_dir` directory. If cleanup also
    /// fails, returns [`PatientError::CleanupAfterInitialiseFailed`] with both the
    /// original error and the cleanup error.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let files = [FileToWrite {
    ///     relative_path: Path::new("STATUS.yaml"),
    ///     content: &status_yaml,
    ///     old_content: None,
    /// }];
    ///
    /// VersionedFileService::init_and_commit(
    ///     &patient_dir,
    ///     &author,
    ///     &commit_message,
    ///     &files,
    /// )?;
    /// ```
    pub(crate) fn init_and_commit(
        patient_dir: &Path,
        author: &Author,
        message: &VprCommitMessage,
        files: &[FileToWrite],
    ) -> PatientResult<()> {
        let result: PatientResult<()> = (|| {
            let _repo = Self::init(patient_dir)?;
            Self::write_and_commit_files(patient_dir, author, message, files)?;
            Ok(())
        })();

        match result {
            Ok(()) => Ok(()),
            Err(init_error) => {
                // Attempt cleanup - remove entire patient_dir
                if let Err(cleanup_err) = cleanup_patient_dir(patient_dir) {
                    return Err(PatientError::CleanupAfterInitialiseFailed {
                        path: patient_dir.to_path_buf(),
                        init_error: Box::new(init_error),
                        cleanup_error: cleanup_err,
                    });
                }
                Err(init_error)
            }
        }
    }

    /// Create a commit with rendered message for the specified paths.
    ///
    /// This is an internal helper that performs the actual commit operation with a
    /// pre-rendered commit message string. It handles path normalisation (absolute to
    /// relative) and validates that paths don't escape the repository directory.
    ///
    /// # Arguments
    ///
    /// * `author` - Author information for commit signature
    /// * `message` - Pre-rendered commit message string
    /// * `relative_paths` - Paths to commit (will be normalised if absolute)
    ///
    /// # Errors
    ///
    /// Returns `PatientError` if:
    /// - HEAD cannot be set to main branch
    /// - Git index operations fail
    /// - Path is outside repository or contains `..`
    /// - Commit creation fails
    fn commit_paths_rendered(
        &self,
        author: &Author,
        message: &str,
        relative_paths: &[PathBuf],
    ) -> PatientResult<git2::Oid> {
        self.ensure_main_head()?;
        let mut index = self.repo.index().map_err(PatientError::GitIndex)?;

        for path in relative_paths {
            // `git2::Index::add_path` requires repo-workdir-relative paths.
            let rel = if path.is_absolute() {
                path.strip_prefix(&self.workdir)
                    .map_err(|_| {
                        PatientError::InvalidInput(
                            "path is outside the repository working directory".into(),
                        )
                    })?
                    .to_path_buf()
            } else {
                path.to_path_buf()
            };

            if rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(PatientError::InvalidInput(
                    "path must not contain parent directory references (..)".into(),
                ));
            }

            index.add_path(&rel).map_err(PatientError::GitAdd)?;
        }

        self.commit_from_index(author, message, &mut index)
    }

    /// Create a commit from the current Git index state.
    ///
    /// This is the lowest-level commit creation helper. It validates author information,
    /// writes the index as a tree, and creates either a signed or unsigned commit depending
    /// on whether the author has a signature key.
    ///
    /// For signed commits, this method:
    /// 1. Creates the unsigned commit buffer with correct parent list
    /// 2. Signs the buffer using ECDSA P-256
    /// 3. Validates certificate matches signing key (if certificate provided)
    /// 4. Creates the signed commit and manually updates refs
    ///
    /// # Arguments
    ///
    /// * `author` - Validated author information
    /// * `message` - Complete commit message text
    /// * `index` - Git index containing staged changes
    ///
    /// # Errors
    ///
    /// Returns `PatientError` if:
    /// - Author validation fails
    /// - Tree write or lookup fails
    /// - Signature creation fails
    /// - Certificate/key mismatch detected
    /// - Commit creation or ref update fails
    fn commit_from_index(
        &self,
        author: &Author,
        message: &str,
        index: &mut git2::Index,
    ) -> PatientResult<git2::Oid> {
        // Ensure author metadata is valid before creating any commit buffers or signatures.
        author.validate_commit_author()?;

        let tree_id = index.write_tree().map_err(PatientError::GitWriteTree)?;
        let tree = self
            .repo
            .find_tree(tree_id)
            .map_err(PatientError::GitFindTree)?;

        let sig = git2::Signature::now(&author.name, &author.email)
            .map_err(PatientError::GitSignature)?;

        if let Some(private_key_pem) = &author.signature {
            // Create the canonical unsigned commit buffer with correct parent list.
            let parents = self.resolve_head_parents()?;
            let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

            let buf = self
                .repo
                .commit_create_buffer(&sig, &sig, message, &tree, &parent_refs)
                .map_err(PatientError::GitCommitBuffer)?;
            let buf_str = String::from_utf8(buf.as_ref().to_vec())
                .map_err(PatientError::CommitBufferToString)?;

            let key_pem = Self::load_private_key_pem(private_key_pem)?;
            let signing_key = SigningKey::from_pkcs8_pem(&key_pem)
                .map_err(|e| PatientError::EcdsaPrivateKeyParse(Box::new(e)))?;

            let public_key_bytes = signing_key
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec();

            if let Some(cert_bytes) = author.certificate.as_deref() {
                let cert_public_key = extract_cert_public_key_sec1(cert_bytes)?;
                if cert_public_key != public_key_bytes {
                    return Err(PatientError::AuthorCertificatePublicKeyMismatch);
                }
            }

            // Sign the unsigned commit buffer. Signature is raw 64-byte (r||s), base64-encoded.
            let signature: Signature = signing_key.sign(buf_str.as_bytes());

            let payload = VprCommitSignaturePayloadV1 {
                signature: general_purpose::STANDARD.encode(signature.to_bytes()),
                public_key: general_purpose::STANDARD.encode(&public_key_bytes),
                certificate: author
                    .certificate
                    .as_deref()
                    .map(|b| general_purpose::STANDARD.encode(b)),
            };

            let payload_json = serde_json::to_vec(&payload).map_err(PatientError::Serialization)?;
            let signature_str = general_purpose::STANDARD.encode(payload_json);

            let oid = self
                .repo
                .commit_signed(&buf_str, &signature_str, None)
                .map_err(PatientError::GitCommitSigned)?;

            // `commit_signed` creates the object but does not move refs.
            self.repo
                .reference(MAIN_REF, oid, true, "signed commit")
                .map_err(PatientError::GitReference)?;
            self.repo
                .set_head(MAIN_REF)
                .map_err(PatientError::GitSetHead)?;

            Ok(oid)
        } else {
            let parents = self.resolve_head_parents()?;
            let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
            // Normal commit updates HEAD (and underlying ref).
            self.repo
                .commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
                .map_err(PatientError::GitCommit)
        }
    }

    /// Resolve the parent commit(s) for a new commit.
    ///
    /// Determines the appropriate parent list based on repository state:
    /// - If `HEAD` exists and points to a commit, returns that commit as the parent
    /// - If the repository is empty (unborn branch or not found), returns empty parent list
    /// - Other errors are propagated
    ///
    /// This handles the distinction between the first commit (no parents) and subsequent
    /// commits (one parent) in a linear history.
    ///
    /// # Errors
    ///
    /// Returns `PatientError::GitHead` if HEAD lookup fails for reasons other than
    /// unborn branch or not found.
    fn resolve_head_parents(&self) -> PatientResult<Vec<git2::Commit<'_>>> {
        match self.repo.head() {
            Ok(head) => {
                let commit = head.peel_to_commit().map_err(PatientError::GitPeel)?;
                Ok(vec![commit])
            }
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => Ok(vec![]),
            Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(vec![]),
            Err(e) => Err(PatientError::GitHead(e)),
        }
    }

    /// Load an ECDSA private key in PKCS#8 PEM format.
    ///
    /// This method accepts private keys in three formats for compatibility:
    /// 1. Direct PEM string (contains `-----BEGIN` marker)
    /// 2. Filesystem path to a PEM file (must exist)
    /// 3. Base64-encoded PEM string
    ///
    /// The method tries each format in order until one succeeds.
    ///
    /// # Arguments
    ///
    /// * `private_key_pem` - Private key as PEM string, file path, or base64-encoded PEM
    ///
    /// # Errors
    ///
    /// Returns `PatientError::EcdsaPrivateKeyParse` if:
    /// - File cannot be read (if path format)
    /// - Base64 decode fails (if base64 format)
    /// - Result is not valid UTF-8
    fn load_private_key_pem(private_key_pem: &str) -> PatientResult<String> {
        if private_key_pem.contains("-----BEGIN") {
            Ok(private_key_pem.to_string())
        } else if Path::new(private_key_pem).exists() {
            fs::read_to_string(private_key_pem)
                .map_err(|e| PatientError::EcdsaPrivateKeyParse(Box::new(e)))
        } else {
            let decoded = general_purpose::STANDARD
                .decode(private_key_pem)
                .map_err(|e| PatientError::EcdsaPrivateKeyParse(Box::new(e)))?;
            String::from_utf8(decoded).map_err(|e| PatientError::EcdsaPrivateKeyParse(Box::new(e)))
        }
    }

    /// Verifies the ECDSA signature of the latest commit in a patient's Git repository.
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
    /// * `base_dir` - The base directory for the patient records (e.g., clinical or demographics directory).
    /// * `uuid` - The UUID of the patient record as a string.
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
    #[allow(dead_code)]
    pub fn verify_commit_signature(
        base_dir: &Path,
        uuid: &str,
        public_key_pem: &str,
    ) -> PatientResult<bool> {
        let uuid = ShardableUuid::parse(uuid)?;
        let patient_dir = uuid.sharded_dir(base_dir);
        let repo = Self::open(&patient_dir)?;

        let head = repo.repo.head().map_err(PatientError::GitHead)?;
        let commit = head.peel_to_commit().map_err(PatientError::GitPeel)?;

        let embedded = match crate::author::extract_embedded_commit_signature(&commit) {
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
            .repo
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

/// Parse a public key from PEM format or extract it from an X.509 certificate.
///
/// This function handles both raw ECDSA public keys in PEM format and X.509 certificates.
/// It's used during signature verification to parse trusted public keys provided by callers.
///
/// # Arguments
///
/// * `pem_or_cert` - Either a PEM-encoded public key or a PEM/DER-encoded X.509 certificate
///
/// # Returns
///
/// A `VerifyingKey` for ECDSA signature verification.
///
/// # Errors
///
/// Returns `PatientError::EcdsaPublicKeyParse` if parsing fails.
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
static FORCE_CLEANUP_ERROR_FOR_THREADS: LazyLock<Mutex<HashSet<std::thread::ThreadId>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

fn cleanup_patient_dir(patient_dir: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    {
        let current_id = std::thread::current().id();
        let mut guard = FORCE_CLEANUP_ERROR_FOR_THREADS
            .lock()
            .expect("FORCE_CLEANUP_ERROR_FOR_THREADS mutex poisoned");

        if guard.remove(&current_id) {
            return Err(std::io::Error::other("forced cleanup failure (test hook)"));
        }
    }

    std::fs::remove_dir_all(patient_dir)
}

#[cfg(test)]
mod tests {
    use super::ClinicalDomain::*;
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn domain_serialises_lowercase() {
        let s = serde_json::to_string(&VprCommitDomain::Clinical(Record)).unwrap();
        assert_eq!(s, "\"record\"");
    }

    #[test]
    fn action_serialises_lowercase() {
        let s = serde_json::to_string(&VprCommitAction::Create).unwrap();
        assert_eq!(s, "\"create\"");
    }

    #[test]
    fn can_get_domain() {
        let msg = VprCommitMessage::new(
            VprCommitDomain::Clinical(Record),
            VprCommitAction::Create,
            "test",
            "location",
        )
        .unwrap();
        assert_eq!(msg.domain(), VprCommitDomain::Clinical(Record));
    }

    #[test]
    fn can_get_action() {
        let msg = VprCommitMessage::new(
            VprCommitDomain::Clinical(Record),
            VprCommitAction::Create,
            "test",
            "location",
        )
        .unwrap();
        assert_eq!(msg.action(), VprCommitAction::Create);
    }

    #[test]
    fn can_get_summary() {
        let msg = VprCommitMessage::new(
            VprCommitDomain::Clinical(Record),
            VprCommitAction::Create,
            "test summary",
            "location",
        )
        .unwrap();
        assert_eq!(msg.summary(), "test summary");
    }

    #[test]
    fn can_get_trailers() {
        let msg = VprCommitMessage::new(
            VprCommitDomain::Clinical(Record),
            VprCommitAction::Create,
            "test",
            "location",
        )
        .unwrap()
        .with_trailer("Key", "Value")
        .unwrap();
        let trailers = msg.trailers();
        assert_eq!(trailers.len(), 1);
        assert_eq!(trailers[0].key(), "Key");
        assert_eq!(trailers[0].value(), "Value");
    }

    #[test]
    fn into_repo_consumes_and_returns_underlying() {
        let temp_dir = TempDir::new().unwrap();
        let service = VersionedFileService::init(temp_dir.path()).unwrap();
        let repo = service.into_repo();
        let workdir = repo.workdir().expect("repo should have workdir");
        let expected = temp_dir.path();

        // Compare canonicalized paths to handle symlinks (e.g., /var -> /private/var on macOS)
        let workdir_canonical = workdir
            .canonicalize()
            .unwrap_or_else(|_| workdir.to_path_buf());
        let expected_canonical = expected
            .canonicalize()
            .unwrap_or_else(|_| expected.to_path_buf());

        assert_eq!(workdir_canonical, expected_canonical);
    }

    #[test]
    fn verifying_key_from_pem() {
        use p256::pkcs8::EncodePublicKey;

        // Generate a valid ECDSA P-256 key pair
        let signing_key = SigningKey::random(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();

        // Encode the public key as PEM
        let pem = verifying_key
            .to_public_key_pem(p256::pkcs8::LineEnding::LF)
            .expect("Failed to encode public key");

        // Parse it back
        let parsed_key = verifying_key_from_public_key_or_cert_pem(&pem).unwrap();

        // Verify they match
        assert_eq!(
            verifying_key.to_encoded_point(false).as_bytes(),
            parsed_key.to_encoded_point(false).as_bytes()
        );
    }

    #[test]
    fn render_without_trailers_is_single_line() {
        let msg = VprCommitMessage::new(
            VprCommitDomain::Clinical(Record),
            VprCommitAction::Create,
            "Patient record created",
            "St Elsewhere Hospital",
        )
        .unwrap();
        assert_eq!(
            msg.render().unwrap(),
            "record:create: Patient record created\n\nCare-Location: St Elsewhere Hospital"
        );
    }

    #[test]
    fn render_with_trailers_matches_git_trailer_format() {
        let msg = VprCommitMessage::new(
            VprCommitDomain::Clinical(Record),
            VprCommitAction::Create,
            "Patient record created",
            "St Elsewhere Hospital",
        )
        .unwrap()
        .with_trailer("Change-Reason", "Correction")
        .unwrap()
        .with_trailer("Authority", "GMC")
        .unwrap();

        assert_eq!(
            msg.render().unwrap(),
            "record:create: Patient record created\n\nCare-Location: St Elsewhere Hospital\nAuthority: GMC\nChange-Reason: Correction"
        );
    }

    #[test]
    fn render_with_author_includes_care_location_after_author_trailers() {
        let author = Author {
            name: "Test Author".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let msg = VprCommitMessage::new(
            VprCommitDomain::Clinical(Record),
            VprCommitAction::Create,
            "Patient record created",
            "St Elsewhere Hospital",
        )
        .unwrap()
        .with_trailer("Change-Reason", "Init")
        .unwrap();

        assert_eq!(
            msg.render_with_author(&author).unwrap(),
            "record:create: Patient record created\n\nAuthor-Name: Test Author\nAuthor-Role: Clinician\nCare-Location: St Elsewhere Hospital\nChange-Reason: Init"
        );
    }

    #[test]
    fn rejects_multiline_summary() {
        let err = VprCommitMessage::new(
            VprCommitDomain::Clinical(Record),
            VprCommitAction::Create,
            "line1\nline2",
            "St Elsewhere Hospital",
        )
        .unwrap_err();

        assert!(matches!(err, PatientError::InvalidInput(_)));
    }

    #[test]
    fn rejects_missing_care_location() {
        let err = VprCommitMessage::new(
            VprCommitDomain::Clinical(Record),
            VprCommitAction::Create,
            "Patient record created",
            "   ",
        )
        .unwrap_err();

        assert!(matches!(err, PatientError::MissingCareLocation));
    }

    #[test]
    fn rejects_multiline_care_location() {
        let err = VprCommitMessage::new(
            VprCommitDomain::Clinical(Record),
            VprCommitAction::Create,
            "Patient record created",
            "St Elsewhere\nHospital",
        )
        .unwrap_err();

        assert!(matches!(err, PatientError::InvalidCareLocation));
    }

    #[test]
    fn rejects_invalid_trailer_key() {
        let err = VprCommitTrailer::new("Bad:Key", "Value").unwrap_err();
        assert!(matches!(err, PatientError::InvalidInput(_)));
    }
}
