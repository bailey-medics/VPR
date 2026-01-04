//! Git helpers for VPR core.
//!
//! VPR stores patient data as files on disk and versions each patient directory using a
//! local Git repository (`git2`/libgit2). This module centralises commit creation so that:
//!
//! - commit creation is consistent across services (clinical now; demographics later),
//! - commit signing is performed over the correct payload, and
//! - branch/ref behaviour is correct when using `Repository::commit_signed`.
//!
//! ## Branch policy
//!
//! VPR standardises on `refs/heads/main`.
//!
//! libgit2's `commit_signed` creates a commit object but **does not update refs** (no branch
//! movement and no `HEAD` update). For signed commits, this module explicitly updates
//! `refs/heads/main` and points `HEAD` to it.
//!
//! ## Signature format
//!
//! When `Author.signature` is present, VPR signs commits using ECDSA P-256.
//!
//! - Signed payload: the *unsigned commit buffer* produced by `Repository::commit_create_buffer`.
//! - Signature bytes: raw 64 bytes (`r || s`, not DER).
//! - Stored form: base64 of a deterministic container (JSON) passed to `commit_signed` and
//!   written into the commit header field `gpgsig`.
//!
//! The container embeds:
//! - `signature`: base64 of raw 64-byte `r||s`
//! - `public_key`: base64 of SEC1-encoded public key bytes
//! - `certificate` (optional): base64 of the certificate bytes (PEM or DER)
//!
//! The verifier in clinical code (`ClinicalService::verify_commit_signature`) expects this
//! exact scheme.

use crate::{Author, PatientError, PatientResult};
use base64::{engine::general_purpose, Engine as _};
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::pkcs8::DecodePrivateKey;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use x509_parser::prelude::*;

const MAIN_REF: &str = "refs/heads/main";

/// Deterministic container for VPR commit signatures.
///
/// This is serialized to JSON with a stable field order (struct order), then base64-encoded and
/// stored as the `gpgsig` header value via `git2::Repository::commit_signed`.
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

/// Controlled vocabulary for VPR commit message domains.
///
/// These are intentionally small and stable, and should be treated as labels/indexes.
///
/// Safety/intent: Do not include patient identifiers or raw clinical data in commit messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum VprCommitDomain {
    Record,
    Obs,
    Dx,
    Tx,
    Admin,
    Corr,
    Meta,
}

impl VprCommitDomain {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Obs => "obs",
            Self::Dx => "dx",
            Self::Tx => "tx",
            Self::Admin => "admin",
            Self::Corr => "corr",
            Self::Meta => "meta",
        }
    }
}

impl fmt::Display for VprCommitDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Controlled vocabulary for VPR commit message actions.
///
/// Safety/intent: Do not include patient identifiers or raw clinical data in commit messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum VprCommitAction {
    Init,
    Add,
    Update,
    Remove,
    Correct,
    Verify,
    Close,
}

impl VprCommitAction {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Add => "add",
            Self::Update => "update",
            Self::Remove => "remove",
            Self::Correct => "correct",
            Self::Verify => "verify",
            Self::Close => "close",
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
/// Renders as `Key: Value`.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct VprCommitTrailer {
    key: String,
    value: String,
}

impl VprCommitTrailer {
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
            return Err(PatientError::InvalidInput);
        }

        Ok(Self { key, value })
    }

    pub(crate) fn key(&self) -> &str {
        &self.key
    }

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
    summary: String,
    care_location: String,
    trailers: Vec<VprCommitTrailer>,
}

impl VprCommitMessage {
    #[allow(dead_code)]
    pub(crate) fn new(
        domain: VprCommitDomain,
        action: VprCommitAction,
        summary: impl Into<String>,
        care_location: impl Into<String>,
    ) -> PatientResult<Self> {
        let summary = summary.into().trim().to_string();
        if summary.is_empty() || summary.contains(['\n', '\r']) {
            return Err(PatientError::InvalidInput);
        }

        let care_location = care_location.into().trim().to_string();
        if care_location.is_empty() {
            return Err(PatientError::MissingCareLocation);
        }
        if care_location.contains(['\n', '\r']) {
            return Err(PatientError::InvalidCareLocation);
        }

        Ok(Self {
            domain,
            action,
            summary,
            care_location,
            trailers: Vec::new(),
        })
    }

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

    #[allow(dead_code)]
    pub(crate) fn domain(&self) -> VprCommitDomain {
        self.domain
    }

    #[allow(dead_code)]
    pub(crate) fn action(&self) -> VprCommitAction {
        self.action
    }

    #[allow(dead_code)]
    pub(crate) fn summary(&self) -> &str {
        &self.summary
    }

    #[allow(dead_code)]
    pub(crate) fn trailers(&self) -> &[VprCommitTrailer] {
        &self.trailers
    }

    #[allow(dead_code)]
    pub(crate) fn render(&self) -> PatientResult<String> {
        // Defensively validate invariants so `VprCommitMessage` stays safe even if constructed
        // manually (e.g. via struct literal within the crate).
        if self.summary.trim().is_empty() || self.summary.contains(['\n', '\r']) {
            return Err(PatientError::InvalidInput);
        }

        if self.care_location.trim().is_empty() {
            return Err(PatientError::MissingCareLocation);
        }
        if self.care_location.contains(['\n', '\r']) {
            return Err(PatientError::InvalidCareLocation);
        }

        let mut rendered = format!("{}:{}: {}", self.domain, self.action, self.summary.trim());

        // Sort non-reserved trailers deterministically.
        let mut other = self.trailers.clone();
        other.sort_by(|a, b| {
            let a_key = (a.key().trim(), a.value().trim());
            let b_key = (b.key().trim(), b.value().trim());
            a_key.cmp(&b_key)
        });

        rendered.push_str("\n\n");
        rendered.push_str("Care-Location: ");
        rendered.push_str(self.care_location.trim());

        for trailer in other {
            if trailer.key().contains(['\n', '\r'])
                || trailer.key().trim().is_empty()
                || trailer.key().contains(':')
                || trailer.value().contains(['\n', '\r'])
            {
                return Err(PatientError::InvalidInput);
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

        if self.care_location.trim().is_empty() {
            return Err(PatientError::MissingCareLocation);
        }
        if self.care_location.contains(['\n', '\r']) {
            return Err(PatientError::InvalidCareLocation);
        }

        let mut rendered = format!("{}:{}: {}", self.domain, self.action, self.summary.trim());

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
        rendered.push_str(self.care_location.trim());

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
pub(crate) struct GitService {
    repo: git2::Repository,
    workdir: PathBuf,
}

impl GitService {
    /// Create a new repository at `workdir`.
    pub(crate) fn init(workdir: &Path) -> PatientResult<Self> {
        let repo = git2::Repository::init(workdir).map_err(PatientError::GitInit)?;
        Ok(Self {
            repo,
            workdir: workdir.to_path_buf(),
        })
    }

    /// Open an existing repository at `workdir`.
    pub(crate) fn open(workdir: &Path) -> PatientResult<Self> {
        let repo = git2::Repository::open(workdir).map_err(PatientError::GitOpen)?;
        Ok(Self {
            repo,
            workdir: workdir.to_path_buf(),
        })
    }

    /// Consume this wrapper and return the underlying `git2::Repository`.
    ///
    /// This is useful when existing code needs to perform lower-level Git operations.
    pub(crate) fn into_repo(self) -> git2::Repository {
        self.repo
    }

    /// Ensure `HEAD` points at `refs/heads/main`.
    ///
    /// For newly initialised repositories this creates an "unborn" `main` branch until the first
    /// commit is written.
    fn ensure_main_head(&self) -> PatientResult<()> {
        self.repo
            .set_head(MAIN_REF)
            .map_err(PatientError::GitSetHead)?;
        Ok(())
    }

    /// Create a commit including *all* files under the repo workdir.
    ///
    /// Commit messages must use the structured `VprCommitMessage` format.
    pub(crate) fn commit_all(
        &self,
        author: &Author,
        message: &VprCommitMessage,
    ) -> PatientResult<git2::Oid> {
        let rendered = message.render_with_author(author)?;
        let paths = self.collect_paths_recursive()?;
        self.commit_paths_rendered(author, &rendered, &paths)
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
                    .map_err(|_| PatientError::InvalidInput)?
                    .to_path_buf()
            } else {
                path.to_path_buf()
            };

            if rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(PatientError::InvalidInput);
            }

            index.add_path(&rel).map_err(PatientError::GitAdd)?;
        }

        self.commit_from_index(author, message, &mut index)
    }

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
    /// - If `HEAD` exists, the parent list is `[HEAD]`.
    /// - If the repository is empty (`UnbornBranch`/`NotFound`), the parent list is empty.
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

    /// Load an ECDSA private key in PKCS#8 PEM form.
    ///
    /// Current behaviour (intentionally preserved for now):
    ///
    /// - If the string contains a PEM header, treat it as PEM.
    /// - Else if it is an existing filesystem path, read it.
    /// - Else treat it as base64-encoded PEM.
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

    /// Collect all file paths under the repo workdir, relative to the workdir.
    ///
    /// This is used by `commit_all`.
    ///
    /// `.git/` is skipped.
    fn collect_paths_recursive(&self) -> PatientResult<Vec<PathBuf>> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) -> PatientResult<()> {
            for entry in fs::read_dir(dir).map_err(PatientError::FileRead)? {
                let entry = entry.map_err(PatientError::FileRead)?;
                let entry_path = entry.path();

                if entry_path.ends_with(".git") {
                    continue;
                }

                if entry_path.is_dir() {
                    walk(&entry_path, base, out)?;
                } else {
                    let rel = entry_path
                        .strip_prefix(base)
                        .map_err(|_| PatientError::InvalidInput)?;
                    out.push(rel.to_path_buf());
                }
            }
            Ok(())
        }

        let mut paths = Vec::new();
        walk(&self.workdir, &self.workdir, &mut paths)?;
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_serialises_lowercase() {
        let s = serde_json::to_string(&VprCommitDomain::Record).unwrap();
        assert_eq!(s, "\"record\"");
    }

    #[test]
    fn action_serialises_lowercase() {
        let s = serde_json::to_string(&VprCommitAction::Init).unwrap();
        assert_eq!(s, "\"init\"");
    }

    #[test]
    fn render_without_trailers_is_single_line() {
        let msg = VprCommitMessage::new(
            VprCommitDomain::Record,
            VprCommitAction::Init,
            "Patient record created",
            "St Elsewhere Hospital",
        )
        .unwrap();
        assert_eq!(
            msg.render().unwrap(),
            "record:init: Patient record created\n\nCare-Location: St Elsewhere Hospital"
        );
    }

    #[test]
    fn render_with_trailers_matches_git_trailer_format() {
        let msg = VprCommitMessage::new(
            VprCommitDomain::Record,
            VprCommitAction::Init,
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
            "record:init: Patient record created\n\nCare-Location: St Elsewhere Hospital\nAuthority: GMC\nChange-Reason: Correction"
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
            VprCommitDomain::Record,
            VprCommitAction::Init,
            "Patient record created",
            "St Elsewhere Hospital",
        )
        .unwrap()
        .with_trailer("Change-Reason", "Init")
        .unwrap();

        assert_eq!(
            msg.render_with_author(&author).unwrap(),
            "record:init: Patient record created\n\nAuthor-Name: Test Author\nAuthor-Role: Clinician\nCare-Location: St Elsewhere Hospital\nChange-Reason: Init"
        );
    }

    #[test]
    fn rejects_multiline_summary() {
        let err = VprCommitMessage::new(
            VprCommitDomain::Record,
            VprCommitAction::Init,
            "line1\nline2",
            "St Elsewhere Hospital",
        )
        .unwrap_err();

        assert!(matches!(err, PatientError::InvalidInput));
    }

    #[test]
    fn rejects_missing_care_location() {
        let err = VprCommitMessage::new(
            VprCommitDomain::Record,
            VprCommitAction::Init,
            "Patient record created",
            "   ",
        )
        .unwrap_err();

        assert!(matches!(err, PatientError::MissingCareLocation));
    }

    #[test]
    fn rejects_multiline_care_location() {
        let err = VprCommitMessage::new(
            VprCommitDomain::Record,
            VprCommitAction::Init,
            "Patient record created",
            "St Elsewhere\nHospital",
        )
        .unwrap_err();

        assert!(matches!(err, PatientError::InvalidCareLocation));
    }

    #[test]
    fn rejects_invalid_trailer_key() {
        let err = VprCommitTrailer::new("Bad:Key", "Value").unwrap_err();
        assert!(matches!(err, PatientError::InvalidInput));
    }
}
