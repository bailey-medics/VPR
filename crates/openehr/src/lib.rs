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

use uuid::Uuid;

use serde::{Deserialize, Serialize};

pub mod rm_1_1_0;

/// Supported openEHR RM versions.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    type Err = OpenEhrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "rm_1_1_0" => Ok(RmVersion::rm_1_1_0),
            _ => Err(OpenEhrError::UnsupportedRmVersion(s.to_string())),
        }
    }
}

pub struct EhrId(String);

impl EhrId {
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// VPR domain primitive representing an external reference.
///
/// This is intentionally small to avoid coupling this crate to any upstream domain types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalReference {
    /// URI-like namespace identifying the subject system (for example `vpr://vpr.dev.1/mpi`).
    pub namespace: String,
    /// Subject identifier.
    pub id: uuid::Uuid,
}

use thiserror::Error;

/// Errors returned by the `openehr` boundary crate.
#[derive(Debug, Error)]
pub enum OpenEhrError {
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

/// Render an `EHR_STATUS` YAML string for the specified RM version.
///
/// # Arguments
///
/// * `version` - RM version identifier.
/// * `previous_data` - Optional YAML text representing an existing `EHR_STATUS`.
/// * `ehr_id_str` - Optional EHR identifier as a string.
/// * `external_refs` - Optional subject external references.
///
/// # Returns
///
/// Returns a YAML string representation of the EHR_STATUS.
///
/// # Errors
///
/// Returns [`OpenEhrError`] if:
/// - the RM version is not supported,
/// - the previous_data YAML is invalid,
/// - both previous_data and ehr_id_str are None.
pub fn ehr_status_render(
    version: RmVersion,
    previous_data: Option<&str>,
    ehr_id: Option<&EhrId>,
    external_refs: Option<Vec<ExternalReference>>,
) -> Result<String, OpenEhrError> {
    match version {
        RmVersion::rm_1_1_0 => {
            rm_1_1_0::ehr_status::ehr_status_render(previous_data, ehr_id, external_refs)
        }
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
/// Returns [`OpenEhrError`] if the YAML is invalid or does not match the expected wire schema.
pub fn read_ehr_status_yaml(yaml: &str) -> Result<rm_1_1_0::ehr_status::EhrStatus, OpenEhrError> {
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
/// Returns [`OpenEhrError`] if serialisation fails.
pub fn write_ehr_status_yaml(
    component: &rm_1_1_0::ehr_status::EhrStatus,
) -> Result<String, OpenEhrError> {
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
/// Returns [`OpenEhrError`] if the front matter is missing/invalid or parsing fails.
pub fn read_narrative_markdown(
    input: &str,
) -> Result<rm_1_1_0::narrative::NarrativeComponent, OpenEhrError> {
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
/// Returns [`OpenEhrError`] if serialisation fails.
pub fn write_narrative_markdown(
    component: &rm_1_1_0::narrative::NarrativeComponent,
) -> Result<String, OpenEhrError> {
    rm_1_1_0::narrative::write_markdown(component)
}

/// Extract the RM version from a YAML string.
///
/// This function parses the provided YAML string and extracts the `rm_version` field,
/// which should be the first field in VPR YAML documents.
///
/// # Arguments
///
/// * `yaml` - YAML string to parse.
///
/// # Returns
///
/// Returns the RM version found in the YAML.
///
/// # Errors
///
/// Returns [`OpenEhrError`] if:
/// - the YAML is invalid,
/// - the `rm_version` field is missing,
/// - the `rm_version` value cannot be converted to a string,
/// - the version string is not a supported RM version.
pub fn extract_rm_version(yaml: &str) -> Result<RmVersion, OpenEhrError> {
    use serde_yaml::Value;

    // Parse the YAML to validate it's well-formed
    let value: Value = serde_yaml::from_str(yaml)?;

    // Extract the rm_version field and convert it to a string
    let version_str = value
        .get("rm_version")
        .and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
        .ok_or_else(|| {
            OpenEhrError::Translation("rm_version field missing or not a valid value".to_string())
        })?;

    // Convert the string to RmVersion
    version_str.parse::<RmVersion>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_rm_version_extracts_valid_version() {
        let yaml = r#"rm_version: rm_1_1_0
ehr_id:
    value: test-id
"#;

        let version = extract_rm_version(yaml).expect("should extract version");
        assert_eq!(version, RmVersion::rm_1_1_0);
    }

    #[test]
    fn extract_rm_version_rejects_invalid_yaml() {
        let invalid_yaml = r#"rm_version: rm_1_1_0
invalid: yaml: content:
"#;

        let err = extract_rm_version(invalid_yaml).expect_err("should reject invalid YAML");
        match err {
            OpenEhrError::InvalidYaml(_) => {} // Expected
            other => panic!("expected InvalidYaml error, got {:?}", other),
        }
    }

    #[test]
    fn extract_rm_version_rejects_missing_rm_version() {
        let yaml_without_version = r#"ehr_id:
    value: test-id
"#;

        let err =
            extract_rm_version(yaml_without_version).expect_err("should reject missing rm_version");
        match err {
            OpenEhrError::Translation(msg) => {
                assert!(msg.contains("rm_version field missing"));
            }
            other => panic!("expected Translation error, got {:?}", other),
        }
    }

    #[test]
    fn extract_rm_version_rejects_non_string_rm_version() {
        let yaml_with_non_string_version = r#"rm_version: 1.1.0
ehr_id:
    value: test-id
"#;

        let err = extract_rm_version(yaml_with_non_string_version)
            .expect_err("should reject non-string rm_version");
        match err {
            OpenEhrError::UnsupportedRmVersion(version) => {
                assert_eq!(version, "1.1.0");
            }
            other => panic!("expected UnsupportedRmVersion error, got {:?}", other),
        }
    }

    #[test]
    fn extract_rm_version_rejects_unsupported_version() {
        let yaml_with_unsupported_version = r#"rm_version: rm_2_0_0
ehr_id:
    value: test-id
"#;

        let err = extract_rm_version(yaml_with_unsupported_version)
            .expect_err("should reject unsupported version");
        match err {
            OpenEhrError::UnsupportedRmVersion(version) => {
                assert_eq!(version, "rm_2_0_0");
            }
            other => panic!("expected UnsupportedRmVersion error, got {:?}", other),
        }
    }
}
