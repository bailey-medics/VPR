//! Repository-related utilities.
//!
//! This module contains functions for managing patient data repositories,
//! including directory allocation and other repository operations.

use crate::uuid::UuidService;
use crate::{PatientError, PatientResult};
use std::{
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

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
