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

/// Ensure `HEAD` points at `refs/heads/main`.
///
/// For newly initialised repositories this creates an "unborn" `main` branch until the first
/// commit is written.
pub(crate) fn ensure_main_head(repo: &git2::Repository) -> PatientResult<()> {
    repo.set_head(MAIN_REF).map_err(PatientError::GitSetHead)?;
    Ok(())
}

/// Create a commit including *all* files under `workdir` (excluding `.git/`).
///
/// This helper is intended for initial repository creation (e.g., copy templates + write initial
/// files and commit everything).
///
/// If `author.signature` is present, the commit is created via `commit_signed` and `refs/heads/main`
/// (and `HEAD`) are updated to point at the new commit.
pub(crate) fn commit_all(
    repo: &git2::Repository,
    workdir: &Path,
    author: &Author,
    message: &str,
) -> PatientResult<git2::Oid> {
    let paths = collect_paths_recursive(workdir)?;
    commit_paths(repo, workdir, author, message, &paths)
}

/// Create a commit including only the provided file paths (relative to `workdir`).
///
/// This is useful for “surgical” updates where you don’t want to commit everything.
///
/// # Path rules
///
/// `relative_paths` may contain:
///
/// - repo-workdir-relative paths (recommended), or
/// - absolute paths under `workdir` (they will be normalised to relative paths).
///
/// Paths containing `..` are rejected.
pub(crate) fn commit_paths(
    repo: &git2::Repository,
    workdir: &Path,
    author: &Author,
    message: &str,
    relative_paths: &[PathBuf],
) -> PatientResult<git2::Oid> {
    ensure_main_head(repo)?;
    let mut index = repo.index().map_err(PatientError::GitIndex)?;

    for path in relative_paths {
        // `git2::Index::add_path` requires repo-workdir-relative paths.
        let rel = if path.is_absolute() {
            path.strip_prefix(workdir)
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

    commit_from_index(repo, author, message, &mut index)
}

fn commit_from_index(
    repo: &git2::Repository,
    author: &Author,
    message: &str,
    index: &mut git2::Index,
) -> PatientResult<git2::Oid> {
    let tree_id = index.write_tree().map_err(PatientError::GitWriteTree)?;
    let tree = repo.find_tree(tree_id).map_err(PatientError::GitFindTree)?;

    let sig =
        git2::Signature::now(&author.name, &author.email).map_err(PatientError::GitSignature)?;

    if let Some(private_key_pem) = &author.signature {
        // Create the canonical unsigned commit buffer with correct parent list.
        let parents = resolve_head_parents(repo)?;
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

        let buf = repo
            .commit_create_buffer(&sig, &sig, message, &tree, &parent_refs)
            .map_err(PatientError::GitCommitBuffer)?;
        let buf_str =
            String::from_utf8(buf.as_ref().to_vec()).map_err(PatientError::CommitBufferToString)?;

        let key_pem = load_private_key_pem(private_key_pem)?;
        let signing_key = SigningKey::from_pkcs8_pem(&key_pem)
            .map_err(|e| PatientError::EcdsaPrivateKeyParse(Box::new(e)))?;

        // Sign the unsigned commit buffer. Signature is raw 64-byte (r||s), base64-encoded.
        let signature: Signature = signing_key.sign(buf_str.as_bytes());
        let signature_str = general_purpose::STANDARD.encode(signature.to_bytes());

        let oid = repo
            .commit_signed(&buf_str, &signature_str, None)
            .map_err(PatientError::GitCommitSigned)?;

        // `commit_signed` creates the object but does not move refs.
        repo.reference(MAIN_REF, oid, true, "signed commit")
            .map_err(PatientError::GitReference)?;
        repo.set_head(MAIN_REF).map_err(PatientError::GitSetHead)?;

        Ok(oid)
    } else {
        let parents = resolve_head_parents(repo)?;
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
        // Normal commit updates HEAD (and underlying ref).
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
            .map_err(PatientError::GitCommit)
    }
}

/// Resolve the parent commit(s) for a new commit.
///
/// - If `HEAD` exists, the parent list is `[HEAD]`.
/// - If the repository is empty (`UnbornBranch`/`NotFound`), the parent list is empty.
fn resolve_head_parents(repo: &git2::Repository) -> PatientResult<Vec<git2::Commit<'_>>> {
    match repo.head() {
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

/// Collect all file paths under `workdir`, relative to `workdir`.
///
/// This is used by `commit_all`.
///
/// `.git/` is skipped.
fn collect_paths_recursive(workdir: &Path) -> PatientResult<Vec<PathBuf>> {
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
    walk(workdir, workdir, &mut paths)?;
    Ok(paths)
}
