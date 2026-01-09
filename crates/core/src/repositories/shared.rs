//! Shared repository utilities.
//!
//! This module contains shared functions and types for managing patient data repositories,
//! template validation, and file system operations used across different repository types.
//!
//! ## Key Components
//!
//! - **Template Management**: `TemplateDirKind` enum and functions for locating and validating
//!   template directories (`resolve_ehr_template_dir`, `validate_template`)
//! - **Directory Operations**: Utilities for creating unique patient directories
//!   (`create_unique_shared_dir`) and recursive copying (`copy_dir_recursive`)
//! - **Git Integration**: Functions for adding files to Git index (`add_directory_to_index`)

use crate::constants::EHR_TEMPLATE_DIR;
use crate::error::{PatientError, PatientResult};
use crate::uuid::UuidService;
use std::{
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

/// The supported types of repository templates in the VPR system.
///
/// This enum is deliberately *closed* to ensure only known template types
/// are used throughout configuration and validation.
///
/// Each variant may require specific folder structures within its template
/// directory (for example, the clinical template must contain a `.ehr/` subfolder).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateDirKind {
    Clinical,
    Demographics,
    Coordination,
}

impl TemplateDirKind {
    /// Returns the required subfolder (if any) for this template kind.
    pub fn required_subdir(&self) -> Option<&'static str> {
        match self {
            TemplateDirKind::Clinical => Some(".ehr"),
            TemplateDirKind::Demographics => Some("identifiers"),
            TemplateDirKind::Coordination => None,
        }
    }

    /// Returns a human-readable name for this template kind.
    pub fn display_name(&self) -> &'static str {
        match self {
            TemplateDirKind::Clinical => "EHR template directory",
            TemplateDirKind::Demographics => "Demographics template directory",
            TemplateDirKind::Coordination => "Coordination template directory",
        }
    }
}

/// Creates a unique shared directory within the base records directory.
///
/// This function generates UUIDs using the provided source function and attempts to create
/// a corresponding sharded directory. It guards against UUID collisions or pre-existing
/// directories by retrying up to 5 times with different UUIDs.
///
/// # Arguments
///
/// * `base_dir` - The base records directory.
/// * `uuid_source` - A mutable closure that generates new `UuidService` instances.
///
/// # Returns
///
/// Returns a tuple of the allocated `UuidService` and the `PathBuf` to the created directory.
///
/// # Errors
///
/// Returns a `PatientError::PatientDirCreation` if:
/// - directory creation fails after 5 attempts,
/// - parent directory creation fails.
pub(crate) fn create_unique_shared_dir(
    base_dir: &Path,
    mut uuid_source: impl FnMut() -> UuidService,
) -> PatientResult<(UuidService, PathBuf)> {
    // Allocate a new UUID, but guard against pathological UUID collisions (or pre-existing
    // directories from external interference) by limiting retries.
    for _attempt in 0..5 {
        let uuid = uuid_source();
        let candidate = uuid.sharded_dir(base_dir);

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

/// Validates a template directory for safety and correctness.
///
/// This function performs comprehensive validation of template directories:
/// - Checks for required subfolders based on template kind
/// - Scans for symlinks (not allowed)
/// - Enforces file count and size limits
/// - Validates directory depth
///
/// # Arguments
/// * `kind` - The type of template directory being validated
/// * `template_dir` - Path to the template directory
///
/// # Errors
/// Returns `PatientError::InvalidInput` if validation fails
pub fn validate_template(kind: &TemplateDirKind, template_dir: &Path) -> PatientResult<()> {
    const MAX_FILES: usize = 2_000;
    const MAX_TOTAL_BYTES: u64 = 50 * 1024 * 1024; // 50 MiB
    const MAX_DEPTH: usize = 20;

    fn scan_dir(
        path: &Path,
        depth: usize,
        files: &mut usize,
        bytes: &mut u64,
        kind: &TemplateDirKind,
    ) -> PatientResult<()> {
        if depth > MAX_DEPTH {
            return Err(PatientError::InvalidInput(
                "Template directory exceeds maximum nesting depth".into(),
            ));
        }

        for entry in std::fs::read_dir(path).map_err(PatientError::FileRead)? {
            let entry = entry.map_err(PatientError::FileRead)?;
            let entry_path = entry.path();
            let metadata =
                std::fs::symlink_metadata(&entry_path).map_err(PatientError::FileRead)?;
            let file_type = metadata.file_type();

            if file_type.is_symlink() {
                return Err(PatientError::InvalidInput(format!(
                    "{} must not contain symlinks",
                    kind.display_name()
                )));
            }

            if file_type.is_file() {
                *files += 1;
                *bytes += metadata.len();
                if *files > MAX_FILES || *bytes > MAX_TOTAL_BYTES {
                    return Err(PatientError::InvalidInput(format!(
                        "{} exceeds maximum file count or total size",
                        kind.display_name()
                    )));
                }
            } else if file_type.is_dir() {
                scan_dir(&entry_path, depth + 1, files, bytes, kind)?;
            } else {
                return Err(PatientError::InvalidInput(format!(
                    "{} contains unsupported file types",
                    kind.display_name()
                )));
            }
        }
        Ok(())
    }

    if let Some(subdir) = kind.required_subdir() {
        if !template_dir.join(subdir).is_dir() {
            return Err(PatientError::InvalidInput(format!(
                "{} must contain '{subdir}/' subfolder",
                kind.display_name()
            )));
        }
    }

    let mut files = 0usize;
    let mut bytes = 0u64;
    scan_dir(template_dir, 0, &mut files, &mut bytes, kind)
}

// TODO: we might be able to make this generic later for other template types

/// Resolve the EHR template directory without reading environment variables.
///
/// If `override_dir` is provided, it must be a directory and must contain `.ehr/`.
/// Otherwise this searches for `ehr-template/` relative to the current working directory and
/// then walks up from `CARGO_MANIFEST_DIR`.
///
/// # Search Order
///
/// 1. Use `override_dir` if provided and valid
/// 2. Check `./ehr-template/` relative to current working directory
/// 3. Walk up from `CARGO_MANIFEST_DIR` looking for `ehr-template/`
///
/// # Validation
///
/// A valid template directory must:
/// - Be a directory
/// - Contain a `.ehr/` subdirectory
///
/// # Errors
///
/// Returns `PatientError::InvalidInput` if:
/// - `override_dir` is provided but invalid
/// - No valid template directory is found
pub fn resolve_ehr_template_dir(override_dir: Option<PathBuf>) -> PatientResult<PathBuf> {
    fn looks_like_template_dir(path: &Path) -> bool {
        path.join(".ehr").is_dir()
    }

    if let Some(template_dir) = override_dir {
        if template_dir.is_dir() && looks_like_template_dir(&template_dir) {
            return Ok(template_dir);
        }
        return Err(PatientError::InvalidInput(
            "VPR_EHR_TEMPLATE_DIR override is not a valid template directory (must contain .ehr/)"
                .into(),
        ));
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

    Err(PatientError::InvalidInput(
        "could not locate ehr-template/ directory with .ehr/ subfolder".into(),
    ))
}
