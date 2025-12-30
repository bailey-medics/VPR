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
//! - Stored form: base64 of those 64 bytes, passed to `commit_signed` and written into the
//!   commit header field `gpgsig`.
//!
//! The verifier in clinical code (`ClinicalService::verify_commit_signature`) expects this
//! exact scheme.

use crate::{Author, PatientError, PatientResult};
use base64::{engine::general_purpose, Engine as _};
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::pkcs8::DecodePrivateKey;
use std::fs;
use std::path::{Path, PathBuf};

const MAIN_REF: &str = "refs/heads/main";

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
    pub(crate) fn commit_all(&self, author: &Author, message: &str) -> PatientResult<git2::Oid> {
        let paths = self.collect_paths_recursive()?;
        self.commit_paths(author, message, &paths)
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

            // Sign the unsigned commit buffer. Signature is raw 64-byte (r||s), base64-encoded.
            let signature: Signature = signing_key.sign(buf_str.as_bytes());
            let signature_str = general_purpose::STANDARD.encode(signature.to_bytes());

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
