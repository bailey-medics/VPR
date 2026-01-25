//! Shared repository utilities.
//!
//! This module contains shared functions and types for managing patient data repositories
//! and file system operations used across different repository types.
//!
//! ## Key Components
//!
//! - **Directory Operations**: Utilities for creating unique patient directories
//!   (`create_uuid_and_shard_dir`) and recursive copying (`copy_dir_recursive`)
//! - **Git Integration**: Functions for adding files to Git index (`add_directory_to_index`)

use crate::error::{PatientError, PatientResult};
use crate::ShardableUuid;
use std::{
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

/// Creates a unique sharded directory within the base records directory.
///
/// This is the simple production API that generates UUIDs internally.
/// Creates a unique sharded directory with a custom UUID source.
///
/// This version accepts a UUID generator for testing collision handling.
/// Production code should use `create_uuid_and_shard_dir()` instead.
///
/// # Arguments
///
/// * `base_dir` - The base records directory.
/// * `uuid_source` - A mutable closure that generates new `ShardableUuid` instances.
///
/// # Returns
///
/// Returns a tuple of the allocated `ShardableUuid` and the `PathBuf` to the created directory.
///
/// # Errors
///
/// Returns a `PatientError::PatientDirCreation` if:
/// - directory creation fails after 5 attempts,
/// - parent directory creation fails.
pub(crate) fn create_uuid_and_shard_dir_with_source(
    base_dir: &Path,
    mut uuid_source: impl FnMut() -> ShardableUuid,
) -> PatientResult<(ShardableUuid, PathBuf)> {
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

/// Creates a unique sharded directory using an auto-generated UUID.
///
/// Simple wrapper for production use that generates UUIDs internally.
/// For testing with deterministic UUIDs, use `create_uuid_and_shard_dir_with_source()`.
///
/// # Arguments
///
/// * `base_dir` - The base records directory.
///
/// # Returns
///
/// Returns a tuple of the allocated `ShardableUuid` and the `PathBuf` to the created directory.
///
/// # Errors
///
/// Returns a `PatientError::PatientDirCreation` if:
/// - directory creation fails after 5 attempts,
/// - parent directory creation fails.
pub(crate) fn create_uuid_and_shard_dir(
    base_dir: &Path,
) -> PatientResult<(ShardableUuid, PathBuf)> {
    create_uuid_and_shard_dir_with_source(base_dir, ShardableUuid::new)
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
