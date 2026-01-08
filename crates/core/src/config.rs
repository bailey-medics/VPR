//! Core runtime configuration.
//!
//! This module defines configuration that should be resolved once at process startup and then
//! passed into core services. The intent is to avoid reading process-wide environment variables
//! during request handling, which can lead to inconsistent behaviour in multi-threaded runtimes
//! and test harnesses.

use crate::constants::{CLINICAL_DIR_NAME, DEMOGRAPHICS_DIR_NAME, EHR_TEMPLATE_DIR, LATEST_RM};
use crate::{PatientError, PatientResult};
use std::path::{Path, PathBuf};

/// Core configuration resolved at startup.
#[derive(Clone, Debug)]
pub struct CoreConfig {
    patient_data_dir: PathBuf,
    ehr_template_dir: PathBuf,
    rm_system_version: openehr::RmVersion,
    vpr_namespace: String,
}

impl CoreConfig {
    /// Create a new `CoreConfig`.
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

    pub fn patient_data_dir(&self) -> &Path {
        &self.patient_data_dir
    }

    pub fn clinical_dir(&self) -> PathBuf {
        self.patient_data_dir.join(CLINICAL_DIR_NAME)
    }

    pub fn demographics_dir(&self) -> PathBuf {
        self.patient_data_dir.join(DEMOGRAPHICS_DIR_NAME)
    }

    pub fn ehr_template_dir(&self) -> &Path {
        &self.ehr_template_dir
    }

    pub fn rm_system_version(&self) -> openehr::RmVersion {
        self.rm_system_version
    }

    pub fn vpr_namespace(&self) -> &str {
        &self.vpr_namespace
    }
}

/// Resolve the EHR template directory without reading environment variables.
///
/// If `override_dir` is provided, it must be a directory and must contain `.ehr/`.
/// Otherwise this searches for `ehr-template/` relative to the current working directory and
/// then walks up from `CARGO_MANIFEST_DIR`.
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
pub fn rm_system_version_from_env_value(
    value: Option<String>,
) -> PatientResult<openehr::RmVersion> {
    let value = value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let parsed = value.map(|v| v.parse::<openehr::RmVersion>()).transpose()?;

    Ok(parsed.unwrap_or(LATEST_RM))
}
