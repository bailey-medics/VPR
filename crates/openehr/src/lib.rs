//! openEHR wire/boundary support.
//!
//! This crate provides **wire models** and **format/translation helpers** for on-disk,
//! version-controlled clinical record files:
//! - YAML components (for example `EHR_STATUS`)
//!
//! This crate focuses on:
//! - standards alignment (openEHR RM structures),
//! - serialisation/deserialisation,
//! - translation between domain primitives and wire structs,
//! - version dispatch via small facade functions where needed.

use chrono::{DateTime, Utc};
use uuid::Uuid;
use vpr_uuid::TimestampId;

use serde::{Deserialize, Serialize};

pub mod clinical_list;
pub mod data_types;
pub mod rm_1_1_0;
pub mod validation;

// Re-export commonly used validation functions
pub use validation::validate_namespace_uri_safe;

// Re-export public clinical list types
pub use clinical_list::{ClinicalList, ClinicalListItem, CodedConcept};

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

/// Represents an EHR identifier.
///
/// This is a simple wrapper around a string to provide type safety for EHR IDs.
pub struct EhrId(String);

impl EhrId {
    /// Creates an `EhrId` from a UUID.
    ///
    /// # Arguments
    ///
    /// * `uuid` - The UUID to convert to an EHR identifier.
    ///
    /// # Returns
    ///
    /// Returns a new `EhrId` containing the string representation of the UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid.to_string())
    }

    /// Returns the EHR identifier as a string slice.
    ///
    /// # Returns
    ///
    /// Returns a reference to the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Domain primitive representing an external reference.
///
/// This is intentionally small to avoid coupling this crate to any upstream domain types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalReference {
    /// URI-like namespace identifying the subject system (for example `ehr://example.com/mpi`).
    pub namespace: String,
    /// Subject identifier.
    pub id: uuid::Uuid,
}

/// Errors returned by the `openehr` boundary crate.
#[derive(Debug, thiserror::Error)]
pub enum OpenEhrError {
    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("invalid YAML: {0}")]
    InvalidYaml(#[from] serde_yaml::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("translation error: {0}")]
    Translation(String),

    #[error("unsupported RM version: {0}")]
    UnsupportedRmVersion(String),

    #[error("invalid archetype ID: {0}")]
    InvalidArchetypeId(String),
}

/// Type alias for Results that can fail with an [`OpenEhrError`].
pub type OpenEhrResult<T> = Result<T, OpenEhrError>;

/// EHR_STATUS operations.
///
/// This is a zero-sized type used for namespacing EHR_STATUS-related operations.
/// All methods are associated functions that dispatch to version-specific implementations.
pub struct EhrStatus;

impl EhrStatus {
    /// Render an `EHR_STATUS` YAML string for the specified RM version.
    ///
    /// # Arguments
    ///
    /// * `rm_version` - RM version identifier.
    /// * `previous_data` - Optional YAML text representing an existing `EHR_STATUS`.
    /// * `ehr_id` - Optional EHR identifier.
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
    /// - both previous_data and ehr_id are None.
    pub fn render(
        rm_version: RmVersion,
        previous_data: Option<&str>,
        ehr_id: Option<&EhrId>,
        external_refs: Option<Vec<ExternalReference>>,
    ) -> Result<String, OpenEhrError> {
        match rm_version {
            RmVersion::rm_1_1_0 => {
                rm_1_1_0::ehr_status::ehr_status_render(previous_data, ehr_id, external_refs)
            }
        }
    }

    /// Parse an EHR_STATUS from YAML for the specified RM version.
    ///
    /// # Arguments
    ///
    /// * `rm_version` - RM version identifier.
    /// * `yaml_text` - YAML text expected to represent an `EHR_STATUS` mapping.
    ///
    /// # Returns
    ///
    /// Returns a valid EHR_STATUS on success.
    ///
    /// # Errors
    ///
    /// Returns [`OpenEhrError`] if:
    /// - the RM version is not supported,
    /// - the YAML does not represent a valid EHR_STATUS,
    /// - any field has an unexpected type,
    /// - any unknown keys are present.
    pub fn parse(
        rm_version: RmVersion,
        yaml_text: &str,
    ) -> Result<rm_1_1_0::ehr_status::EhrStatus, OpenEhrError> {
        match rm_version {
            RmVersion::rm_1_1_0 => rm_1_1_0::ehr_status::ehr_status_parse(yaml_text),
        }
    }
}

/// Letter composition operations.
///
/// This is a zero-sized type used for namespacing letter-related operations.
/// All methods are associated functions that dispatch to version-specific implementations.
pub struct Letter;

impl Letter {
    /// Parse a letter composition from YAML for the specified RM version.
    ///
    /// # Arguments
    ///
    /// * `rm_version` - RM version identifier.
    /// * `yaml_text` - YAML text expected to represent a `COMPOSITION` (letter) mapping.
    ///
    /// # Returns
    ///
    /// Returns a valid letter composition on success.
    ///
    /// # Errors
    ///
    /// Returns [`OpenEhrError`] if:
    /// - the RM version is not supported,
    /// - the YAML does not represent a valid letter composition,
    /// - any field has an unexpected type,
    /// - any unknown keys are present.
    pub fn composition_parse(
        rm_version: RmVersion,
        yaml_text: &str,
    ) -> Result<rm_1_1_0::letter::Composition, OpenEhrError> {
        match rm_version {
            RmVersion::rm_1_1_0 => rm_1_1_0::letter::composition_parse(yaml_text),
        }
    }

    /// Render a letter composition as YAML for the specified RM version.
    ///
    /// # Arguments
    ///
    /// * `rm_version` - RM version identifier.
    /// * `previous_data` - Optional YAML text representing an existing letter.
    /// * `uid` - Optional timestamp-based unique identifier.
    /// * `composer_name` - Optional composer name to update.
    /// * `composer_role` - Optional composer role to update.
    /// * `start_time` - Optional start time to update as a UTC datetime.
    ///
    /// # Returns
    ///
    /// Returns a YAML string representation of the letter.
    ///
    /// # Errors
    ///
    /// Returns [`OpenEhrError`] if:
    /// - the RM version is not supported,
    /// - the previous_data YAML is invalid,
    /// - required fields are missing when creating a new letter.
    pub fn composition_render(
        rm_version: RmVersion,
        previous_data: Option<&str>,
        uid: Option<&TimestampId>,
        composer_name: Option<&str>,
        composer_role: Option<&str>,
        start_time: Option<DateTime<Utc>>,
        clinical_lists: Option<&[ClinicalList]>,
    ) -> Result<String, OpenEhrError> {
        match rm_version {
            RmVersion::rm_1_1_0 => rm_1_1_0::letter::composition_render(
                previous_data,
                uid,
                composer_name,
                composer_role,
                start_time,
                clinical_lists,
            ),
        }
    }
}

/// Extract the RM version from a YAML string.
///
/// This function parses the provided YAML string and extracts the `rm_version` field,
/// which should be the first field in openEHR YAML documents.
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
    fn rm_version_as_str_returns_correct_string() {
        assert_eq!(RmVersion::rm_1_1_0.as_str(), "rm_1_1_0");
    }

    #[test]
    fn rm_version_from_str_parses_valid_version() {
        let version = "rm_1_1_0"
            .parse::<RmVersion>()
            .expect("should parse valid version");
        assert_eq!(version, RmVersion::rm_1_1_0);
    }

    #[test]
    fn rm_version_from_str_rejects_invalid_version() {
        let err = "invalid_version"
            .parse::<RmVersion>()
            .expect_err("should reject invalid version");
        match err {
            OpenEhrError::UnsupportedRmVersion(version) => {
                assert_eq!(version, "invalid_version");
            }
            other => panic!("expected UnsupportedRmVersion error, got {:?}", other),
        }
    }

    #[test]
    fn ehr_id_from_uuid_creates_correct_id() {
        let uuid = Uuid::parse_str("12345678-1234-1234-1234-123456789abc").unwrap();
        let ehr_id = EhrId::from_uuid(uuid);
        assert_eq!(ehr_id.as_str(), "12345678-1234-1234-1234-123456789abc");
    }

    #[test]
    fn ehr_id_as_str_returns_underlying_string() {
        let ehr_id = EhrId("test-id".to_string());
        assert_eq!(ehr_id.as_str(), "test-id");
    }

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
