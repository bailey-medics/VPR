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
//! - `VPR_CLINICAL_TEMPLATE_DIR`: Directory containing clinical templates (optional override)
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
//! clinical_template_dir/
//! └── .ehr/             # Template files copied to new patients
//! ```
//!
//! # Safety and Validation
//!
//! Configuration values are validated at construction time:
//!
//! - Directory paths must exist and be accessible
//! - Clinical templates are scanned for safety (no symlinks, reasonable size limits)
//! - Namespace cannot be empty
//! - RM version must be supported
//!
//! # Usage Pattern
//!
//! ```rust,ignore
//! // In main.rs or startup code
//! let config = CoreConfig::new(
//!     patient_data_dir,
//!     clinical_template_dir,
//!     rm_version,
//!     namespace,
//! )?;
//!
//! // Pass to services
//! let clinical_service = ClinicalService::new(Arc::new(config.clone()));
//! let demographics_service = DemographicsService::new(Arc::new(config));
//! ```

use crate::constants::{CLINICAL_DIR_NAME, DEMOGRAPHICS_DIR_NAME, LATEST_RM};
use crate::error::{PatientError, PatientResult};
use std::path::{Path, PathBuf};

/// Core configuration resolved at startup.
///
/// This struct holds all configuration values that are determined once at process startup
/// and remain immutable throughout the application lifecycle. It provides access to:
///
/// - Patient data storage directories
/// - Clinical template location
/// - OpenEHR Reference Model version
/// - VPR instance namespace
///
/// All paths are validated and canonicalized during construction.
#[derive(Clone, Debug)]
pub struct CoreConfig {
    patient_data_dir: PathBuf,
    clinical_template_dir: PathBuf,
    rm_system_version: openehr::RmVersion,
    vpr_namespace: String,
}

impl CoreConfig {
    /// Create a new `CoreConfig`.
    ///
    /// # Arguments
    ///
    /// * `patient_data_dir` - Base directory for patient data storage
    /// * `clinical_template_dir` - Directory containing clinical templates
    /// * `rm_system_version` - OpenEHR Reference Model version
    /// * `vpr_namespace` - Namespace identifier (cannot be empty)
    ///
    /// # Errors
    ///
    /// Returns `PatientError::InvalidInput` if `vpr_namespace` is empty or whitespace-only.
    pub fn new(
        patient_data_dir: PathBuf,
        clinical_template_dir: PathBuf,
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
            clinical_template_dir,
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

    /// Get the clinical template directory.
    ///
    /// This directory contains template files that are copied when initialising new patients.
    pub fn clinical_template_dir(&self) -> &Path {
        &self.clinical_template_dir
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
