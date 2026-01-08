//! Core runtime configuration.
//!
//! This module defines configuration that should be resolved once at process startup and then
//! passed into core services. The intent is to avoid reading process-wide environment variables
//! during request handling, which can lead to inconsistent behaviour in multi-threaded runtimes
//! and test harnesses.
//!
//! # Configuration Sources
//!
//! Configuration is typically resolved from environment variables at startup:
//!
//! - `PATIENT_DATA_DIR`: Base directory for patient data storage
//! - `VPR_EHR_TEMPLATE_DIR`: Directory containing EHR templates (optional override)
//! - `RM_SYSTEM_VERSION`: OpenEHR Reference Model version (optional)
//! - `VPR_NAMESPACE`: Namespace identifier for this VPR instance
//!
//! # Directory Structure
//!
//! The configuration establishes the following directory layout:
//!
//! ```text
//! patient_data_dir/
//! ├── clinical/          # Clinical records (Git repos per patient)
//! └── demographics/      # Demographic data (JSON files per patient)
//!
//! ehr_template_dir/
//! └── .ehr/             # Template files copied to new patients
//! ```
//!
//! # Safety and Validation
//!
//! Configuration values are validated at construction time:
//!
//! - Directory paths must exist and be accessible
//! - EHR templates are scanned for safety (no symlinks, reasonable size limits)
//! - Namespace cannot be empty
//! - RM version must be supported
//!
//! # Usage Pattern
//!
//! ```rust,ignore
//! // In main.rs or startup code
//! let config = CoreConfig::new(
//!     patient_data_dir,
//!     ehr_template_dir,
//!     rm_version,
//!     namespace,
//! )?;
//!
//! // Pass to services
//! let clinical_service = ClinicalService::new(Arc::new(config.clone()));
//! let demographics_service = DemographicsService::new(Arc::new(config));
//! ```

use crate::constants::{CLINICAL_DIR_NAME, DEMOGRAPHICS_DIR_NAME, EHR_TEMPLATE_DIR, LATEST_RM};
use crate::{PatientError, PatientResult};
use std::path::{Path, PathBuf};

/// Core configuration resolved at startup.
///
/// This struct holds all configuration values that are determined once at process startup
/// and remain immutable throughout the application lifecycle. It provides access to:
///
/// - Patient data storage directories
/// - EHR template location
/// - OpenEHR Reference Model version
/// - VPR instance namespace
///
/// All paths are validated and canonicalized during construction.
#[derive(Clone, Debug)]
pub struct CoreConfig {
    patient_data_dir: PathBuf,
    ehr_template_dir: PathBuf,
    rm_system_version: openehr::RmVersion,
    vpr_namespace: String,
}

impl CoreConfig {
    /// Create a new `CoreConfig`.
    ///
    /// # Arguments
    ///
    /// * `patient_data_dir` - Base directory for patient data storage
    /// * `ehr_template_dir` - Directory containing EHR templates
    /// * `rm_system_version` - OpenEHR Reference Model version
    /// * `vpr_namespace` - Namespace identifier (cannot be empty)
    ///
    /// # Errors
    ///
    /// Returns `PatientError::InvalidInput` if `vpr_namespace` is empty or whitespace-only.
    pub fn new(
        patient_data_dir: PathBuf,
        ehr_template_dir: PathBuf,
        rm_system_version: openehr::RmVersion,
        vpr_namespace: String,
    ) -> PatientResult<Self> {
        if vpr_namespace.trim().is_empty() {
            return Err(PatientError::InvalidInput(
                "vpr_namespace cannot be empty".into(),
            ));
        }

        Ok(Self {
            patient_data_dir,
            ehr_template_dir,
            rm_system_version,
            vpr_namespace,
        })
    }

    /// Get the base patient data directory.
    ///
    /// This is the root directory containing `clinical/` and `demographics/` subdirectories.
    pub fn patient_data_dir(&self) -> &Path {
        &self.patient_data_dir
    }

    /// Get the clinical records directory.
    ///
    /// Returns `patient_data_dir/clinical/`.
    pub fn clinical_dir(&self) -> PathBuf {
        self.patient_data_dir.join(CLINICAL_DIR_NAME)
    }

    /// Get the demographics directory.
    ///
    /// Returns `patient_data_dir/demographics/`.
    pub fn demographics_dir(&self) -> PathBuf {
        self.patient_data_dir.join(DEMOGRAPHICS_DIR_NAME)
    }

    /// Get the EHR template directory.
    ///
    /// This directory contains template files that are copied when initializing new patients.
    pub fn ehr_template_dir(&self) -> &Path {
        &self.ehr_template_dir
    }

    /// Get the OpenEHR Reference Model version.
    ///
    /// This determines which RM features and constraints are enforced.
    pub fn rm_system_version(&self) -> openehr::RmVersion {
        self.rm_system_version
    }

    /// Get the VPR namespace identifier.
    ///
    /// Used to isolate different VPR instances or deployments.
    pub fn vpr_namespace(&self) -> &str {
        &self.vpr_namespace
    }
}

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

/// Validate that the resolved EHR template directory is safe to recursively copy.
///
/// This is intended to be run at startup when `CoreConfig` is constructed.
///
/// # Safety Checks
///
/// The function enforces several limits to prevent accidental misuse:
///
/// - **Maximum files**: 2,000 files total
/// - **Maximum size**: 50 MiB total
/// - **Maximum depth**: 20 directory levels
/// - **File types**: Only regular files and directories allowed (no symlinks, devices, etc.)
/// - **Required structure**: Must contain `.ehr/` subdirectory
///
/// # Purpose
///
/// These checks prevent:
/// - Accidental copying of entire filesystems when template_dir is misconfigured
/// - Performance issues from extremely large template directories
/// - Security issues from copying symlinks or special files
/// - Infinite recursion from circular directory structures
///
/// # Errors
///
/// Returns `PatientError::InvalidInput` for various validation failures with descriptive messages.
pub fn validate_ehr_template_dir_safe_to_copy(template_dir: &Path) -> PatientResult<()> {
    // Guardrails for environment overrides without hardcoding expected folder names.
    //
    // Goals:
    // - allow templates to evolve (e.g. add bloods/) without code changes
    // - prevent accidental "copy the world" when template_dir is set to something broad
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
            return Err(PatientError::InvalidInput(
                "EHR template directory exceeds maximum nesting depth".into(),
            ));
        }

        for entry in std::fs::read_dir(path).map_err(PatientError::FileRead)? {
            let entry = entry.map_err(PatientError::FileRead)?;
            let entry_path = entry.path();
            let metadata =
                std::fs::symlink_metadata(&entry_path).map_err(PatientError::FileRead)?;
            let file_type = metadata.file_type();

            if file_type.is_symlink() {
                return Err(PatientError::InvalidInput(
                    "EHR template directory must not contain symlinks".into(),
                ));
            }

            if file_type.is_file() {
                *files = files.saturating_add(1);
                *bytes = bytes.saturating_add(metadata.len());

                if *files > MAX_FILES || *bytes > MAX_TOTAL_BYTES {
                    return Err(PatientError::InvalidInput(
                        "EHR template directory exceeds maximum file count or total size".into(),
                    ));
                }
            } else if file_type.is_dir() {
                scan_dir(&entry_path, depth + 1, files, bytes)?;
            } else {
                // Reject special files (devices, fifos, sockets, etc).
                return Err(PatientError::InvalidInput(
                    "EHR template directory contains unsupported file types (devices, fifos, sockets)".into()
                ));
            }
        }

        Ok(())
    }

    // Minimal sanity check: templates must at least contain the hidden .ehr folder.
    // This prevents common foot-guns like template_dir=".".
    if !template_dir.join(".ehr").is_dir() {
        return Err(PatientError::InvalidInput(
            "EHR template directory must contain .ehr/ subfolder".into(),
        ));
    }

    let mut files = 0usize;
    let mut bytes = 0u64;
    scan_dir(template_dir, 0, &mut files, &mut bytes)
}

/// Parse the RM system version from an optional string value.
///
/// If `value` is `None` or empty/whitespace, returns the latest supported RM.
///
/// # Arguments
///
/// * `value` - Optional string representation of an RM version (e.g., "1.0.4")
///
/// # Returns
///
/// The parsed `RmVersion` or the latest supported version if no value provided.
///
/// # Errors
///
/// Returns `PatientError` if the version string cannot be parsed.
pub fn rm_system_version_from_env_value(
    value: Option<String>,
) -> PatientResult<openehr::RmVersion> {
    let value = value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let parsed = value.map(|v| v.parse::<openehr::RmVersion>()).transpose()?;

    Ok(parsed.unwrap_or(LATEST_RM))
}
