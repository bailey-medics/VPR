//! openEHR wire/boundary support.
//!
//! This crate provides **wire models** and **format/translation helpers** for VPR’s on-disk,
//! Git-backed clinical record files:
//! - YAML components (for example `EHR_STATUS`)
//! - Markdown with YAML front matter (narrative components)
//!
//! Clinical meaning and business rules live in `vpr-core`. This crate focuses on:
//! - standards alignment (openEHR RM structures),
//! - serialisation/deserialisation,
//! - translation between VPR domain primitives and wire structs,
//! - version dispatch via small facade functions where needed.

pub mod rm_1_1_0;

/// Supported openEHR RM versions.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum RmVersion {
    /// openEHR RM 1.1.0.
    rm_1_1_0,
}

impl RmVersion {
    /// Return the canonical string identifier for this RM version.
    pub const fn as_str(self) -> &'static str {
        match self {
            RmVersion::rm_1_1_0 => "rm_1_1_0",
        }
    }
}

impl std::str::FromStr for RmVersion {
    type Err = OpenehrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "rm_1_1_0" => Ok(RmVersion::rm_1_1_0),
            _ => Err(OpenehrError::UnsupportedRmVersion(s.to_string())),
        }
    }
}

/// VPR domain primitive representing an external subject reference.
///
/// This is intentionally small to avoid coupling this crate to any upstream domain types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubjectExternalRef {
    /// URI-like namespace identifying the subject system (for example `vpr://vpr.dev.1/mpi`).
    pub namespace: String,
    /// Subject identifier.
    pub id: uuid::Uuid,
}

use thiserror::Error;

/// Errors returned by the `openehr` boundary crate.
#[derive(Debug, Error)]
pub enum OpenehrError {
    #[error("invalid YAML: {0}")]
    InvalidYaml(#[from] serde_yaml::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("missing YAML front matter header (expected '---' as first line)")]
    MissingFrontMatter,

    #[error("unterminated YAML front matter (missing closing '---' line)")]
    UnterminatedFrontMatter,

    #[error("front matter must be a YAML mapping")]
    FrontMatterNotMapping,

    #[error("invalid UTF-8 or text structure")]
    InvalidText,

    #[error("translation error: {0}")]
    Translation(String),

    #[error("unsupported RM version: {0}")]
    UnsupportedRmVersion(String),
}

/// Write an `EHR_STATUS` YAML file for the specified RM version.
///
/// # Arguments
///
/// * `version` - RM version identifier (for example `"rm_1_1_0"`).
/// * `filename` - Path to the target `ehr_status.yaml` file.
/// * `ehr_id` - EHR identifier for the record.
/// * `subject` - Optional subject external references.
///
/// # Returns
///
/// Returns `Ok(())` on success.
///
/// # Errors
///
/// Returns [`OpenehrError`] if:
/// - the RM version is not supported,
/// - reading/writing the target file fails,
/// - YAML serialisation/deserialisation fails.
pub fn ehr_status_write(
    version: RmVersion,
    filename: &std::path::Path,
    ehr_id: uuid::Uuid,
    subject: Option<Vec<SubjectExternalRef>>,
) -> Result<(), OpenehrError> {
    match version {
        RmVersion::rm_1_1_0 => rm_1_1_0::ehr_status::ehr_status_write(filename, ehr_id, subject),
    }
}

/// Read an RM 1.1.0 `EHR_STATUS` component from YAML.
///
/// # Arguments
///
/// * `yaml` - YAML document containing an `EHR_STATUS` component.
///
/// # Returns
///
/// Returns a parsed RM 1.1.0 wire struct.
///
/// # Errors
///
/// Returns [`OpenehrError`] if the YAML is invalid or does not match the expected wire schema.
pub fn read_ehr_status_yaml(yaml: &str) -> Result<rm_1_1_0::ehr_status::EhrStatus, OpenehrError> {
    rm_1_1_0::ehr_status::read_yaml(yaml)
}

/// Write an RM 1.1.0 `EHR_STATUS` component to YAML.
///
/// # Arguments
///
/// * `component` - RM 1.1.0 wire struct to serialise.
///
/// # Returns
///
/// Returns a YAML string.
///
/// # Errors
///
/// Returns [`OpenehrError`] if serialisation fails.
pub fn write_ehr_status_yaml(
    component: &rm_1_1_0::ehr_status::EhrStatus,
) -> Result<String, OpenehrError> {
    rm_1_1_0::ehr_status::write_yaml(component)
}

/// Read an RM 1.1.0 narrative component from Markdown with YAML front matter.
///
/// # Arguments
///
/// * `input` - Markdown document with YAML front matter.
///
/// # Returns
///
/// Returns a parsed narrative wire struct.
///
/// # Errors
///
/// Returns [`OpenehrError`] if the front matter is missing/invalid or parsing fails.
pub fn read_narrative_markdown(
    input: &str,
) -> Result<rm_1_1_0::narrative::NarrativeComponent, OpenehrError> {
    rm_1_1_0::narrative::read_markdown(input)
}

/// Write an RM 1.1.0 narrative component to Markdown with YAML front matter.
///
/// # Arguments
///
/// * `component` - Narrative wire struct to serialise.
///
/// # Returns
///
/// Returns Markdown with YAML front matter.
///
/// # Errors
///
/// Returns [`OpenehrError`] if serialisation fails.
pub fn write_narrative_markdown(
    component: &rm_1_1_0::narrative::NarrativeComponent,
) -> Result<String, OpenehrError> {
    rm_1_1_0::narrative::write_markdown(component)
}
