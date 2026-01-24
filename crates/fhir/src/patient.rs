//! FHIR-aligned patient wire models and translation helpers.
//!
//! This module provides both domain-level types and wire models for patient resources,
//! which represents patient demographics and identification information.
//!
//! Responsibilities:
//! - Define public domain-level types for external API use
//! - Define a strict wire model for serialisation/deserialisation
//! - Provide translation helpers between domain primitives and the wire model
//! - Validate patient structure and enforce required fields
//!
//! Notes:
//! - This patient file is mutable and overwriteable
//! - Changes should be git-audited where appropriate

use crate::FhirError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use vpr_uuid::ShardableUuid;

// ============================================================================
// Public domain-level types
// ============================================================================

/// Purpose of a human name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameUse {
    /// Official name.
    Official,
    /// Usual/preferred name.
    Usual,
    /// Temporary name.
    Temp,
    /// Nickname or informal name.
    Nickname,
    /// Anonymous name.
    Anonymous,
    /// Old name (no longer in use).
    Old,
    /// Maiden name.
    Maiden,
}

impl NameUse {
    /// Convert to FHIR wire format string.
    fn to_wire(self) -> &'static str {
        match self {
            NameUse::Official => "official",
            NameUse::Usual => "usual",
            NameUse::Temp => "temp",
            NameUse::Nickname => "nickname",
            NameUse::Anonymous => "anonymous",
            NameUse::Old => "old",
            NameUse::Maiden => "maiden",
        }
    }

    /// Parse from FHIR wire format string.
    fn from_wire(s: &str) -> Option<Self> {
        match s {
            "official" => Some(NameUse::Official),
            "usual" => Some(NameUse::Usual),
            "temp" => Some(NameUse::Temp),
            "nickname" => Some(NameUse::Nickname),
            "anonymous" => Some(NameUse::Anonymous),
            "old" => Some(NameUse::Old),
            "maiden" => Some(NameUse::Maiden),
            _ => None,
        }
    }
}

/// Domain-level carrier for patient data (flat structure).
///
/// This struct represents patient demographics in a flat format suitable for
/// direct use in APIs and services. The wire format supports multiple names,
/// but this flat structure extracts the first (primary) name for simplicity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatientData {
    /// Unique identifier for this patient record.
    pub id: ShardableUuid,

    /// Purpose of the name (official, usual, nickname, etc.).
    pub use_type: Option<NameUse>,

    /// Family name (surname).
    pub family: Option<String>,

    /// Given names (first name, middle names).
    pub given: Vec<String>,

    /// Patient's date of birth (ISO 8601 date format: YYYY-MM-DD).
    pub birth_date: Option<String>,

    /// Last updated timestamp.
    pub last_updated: Option<DateTime<Utc>>,
}

// ============================================================================
// Internal nested types
// ============================================================================

/// Internal nested representation of patient data.
///
/// This matches the FHIR wire format structure with nested name and meta objects.
/// Used internally for wire format conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PatientDataNested {
    pub id: ShardableUuid,
    pub names: Vec<FullName>,
    pub birth_date: Option<String>,
    pub meta: Option<PatientMeta>,
}

/// Human name representation (internal).
#[derive(Clone, Debug, PartialEq, Eq)]
struct FullName {
    pub use_type: Option<NameUse>,
    pub family: Option<String>,
    pub given: Vec<String>,
}

/// Patient resource metadata (internal).
#[derive(Clone, Debug, PartialEq, Eq)]
struct PatientMeta {
    pub last_updated: Option<DateTime<Utc>>,
}

// ============================================================================
// Public Patient operations
// ============================================================================

/// Patient resource operations.
///
/// This is a zero-sized type used for namespacing patient-related operations.
/// All methods are associated functions.
pub struct Patient;

impl Patient {
    /// Parse a patient resource from YAML text.
    ///
    /// This uses `serde_path_to_error` to surface a best-effort "path" (e.g. `name.0.family`)
    /// to the failing field when the YAML does not match the wire schema.
    ///
    /// # Arguments
    ///
    /// * `yaml_text` - YAML text expected to represent a patient resource.
    ///
    /// # Returns
    ///
    /// Returns a [`PatientData`] with domain-level fields extracted from the resource.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError`] if:
    /// - the YAML does not represent a valid patient resource,
    /// - any field has an unexpected type,
    /// - any unknown keys are present (due to `#[serde(deny_unknown_fields)]`),
    /// - resourceType is not "Patient".
    pub fn parse(yaml_text: &str) -> Result<PatientData, FhirError> {
        let deserializer = serde_yaml::Deserializer::from_str(yaml_text);

        let wire = match serde_path_to_error::deserialize::<_, PatientWire>(deserializer) {
            Ok(parsed) => parsed,
            Err(err) => {
                let path = err.path().to_string();
                let source = err.into_inner();
                let path = if path.is_empty() {
                    "<root>"
                } else {
                    path.as_str()
                };
                return Err(FhirError::Translation(format!(
                    "Patient schema mismatch at {path}: {source}"
                )));
            }
        };

        // Validate resourceType
        if wire.resource_type != "Patient" {
            return Err(FhirError::InvalidInput(format!(
                "Expected resourceType 'Patient', got '{}'",
                wire.resource_type
            )));
        }

        // Convert wire format to domain types
        wire_to_domain(wire)
    }

    /// Render a patient resource as YAML text.
    ///
    /// This converts domain-level [`PatientData`] into wire format and serializes to YAML.
    ///
    /// # Arguments
    ///
    /// * `data` - Patient data containing all fields.
    ///
    /// # Returns
    ///
    /// Returns a YAML string representation of the patient resource.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError`] if serialisation fails.
    pub fn render(data: &PatientData) -> Result<String, FhirError> {
        let wire: PatientWire = domain_to_wire(data);
        serde_yaml::to_string(&wire)
            .map_err(|e| FhirError::Translation(format!("Failed to serialise patient: {e}")))
    }
}

// ============================================================================
// Wire types (internal)
// ============================================================================

/// Wire representation of a patient resource for on-disk YAML.
///
/// This is the exact structure that will be serialised to/from YAML.
/// All fields use `#[serde(deny_unknown_fields)]` for strict validation.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct PatientWire {
    #[serde(rename = "resourceType")]
    pub resource_type: String,

    pub id: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub name: Vec<FullNameWire>,

    #[serde(rename = "birthDate", skip_serializing_if = "Option::is_none")]
    pub birth_date: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<PatientMetaWire>,
}

/// Wire representation of a human name.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct FullNameWire {
    #[serde(rename = "use", skip_serializing_if = "Option::is_none")]
    pub use_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub given: Vec<String>,
}

/// Wire representation of patient metadata.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct PatientMetaWire {
    #[serde(rename = "lastUpdated", skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

// ============================================================================
// Helper functions (internal)
// ============================================================================

/// Convert wire format to nested internal format.
fn wire_to_nested(wire: PatientWire) -> Result<PatientDataNested, FhirError> {
    let id = ShardableUuid::parse(&wire.id)
        .map_err(|e| FhirError::Translation(format!("Invalid patient ID: {e}")))?;

    let names = wire
        .name
        .into_iter()
        .map(|n| {
            let use_type = n.use_type.as_deref().and_then(NameUse::from_wire);
            Ok(FullName {
                use_type,
                family: n.family,
                given: n.given,
            })
        })
        .collect::<Result<Vec<_>, FhirError>>()?;

    let meta = wire.meta.map(|m| {
        let last_updated = m
            .last_updated
            .as_ref()
            .and_then(|s| s.parse::<DateTime<Utc>>().ok());
        PatientMeta { last_updated }
    });

    Ok(PatientDataNested {
        id,
        names,
        birth_date: wire.birth_date,
        meta,
    })
}

/// Convert nested internal format to wire format.
fn nested_to_wire(nested: &PatientDataNested) -> PatientWire {
    PatientWire {
        resource_type: "Patient".to_string(),
        id: nested.id.to_string(),
        name: nested
            .names
            .iter()
            .map(|n| FullNameWire {
                use_type: n.use_type.map(|u| u.to_wire().to_string()),
                family: n.family.clone(),
                given: n.given.clone(),
            })
            .collect(),
        birth_date: nested.birth_date.clone(),
        meta: nested.meta.as_ref().map(|m| PatientMetaWire {
            last_updated: m.last_updated.map(|dt| dt.to_rfc3339()),
        }),
    }
}

/// Convert flat public PatientData to nested internal format.
fn flat_to_nested(data: &PatientData) -> PatientDataNested {
    let names = if data.use_type.is_some() || data.family.is_some() || !data.given.is_empty() {
        vec![FullName {
            use_type: data.use_type,
            family: data.family.clone(),
            given: data.given.clone(),
        }]
    } else {
        vec![]
    };

    let meta = data.last_updated.map(|lu| PatientMeta {
        last_updated: Some(lu),
    });

    PatientDataNested {
        id: data.id.clone(),
        names,
        birth_date: data.birth_date.clone(),
        meta,
    }
}

/// Convert nested internal format to flat public PatientData.
fn nested_to_flat(nested: PatientDataNested) -> PatientData {
    // Extract first name if available
    let first_name = nested.names.first();

    PatientData {
        id: nested.id,
        use_type: first_name.and_then(|n| n.use_type),
        family: first_name.and_then(|n| n.family.clone()),
        given: first_name.map(|n| n.given.clone()).unwrap_or_default(),
        birth_date: nested.birth_date,
        last_updated: nested.meta.and_then(|m| m.last_updated),
    }
}

/// Convert wire format patient to flat domain type.
fn wire_to_domain(wire: PatientWire) -> Result<PatientData, FhirError> {
    let nested = wire_to_nested(wire)?;
    Ok(nested_to_flat(nested))
}

/// Convert flat domain type to wire format patient.
fn domain_to_wire(data: &PatientData) -> PatientWire {
    let nested = flat_to_nested(data);
    nested_to_wire(&nested)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_sample_yaml() {
        let input = r#"resourceType: Patient
id: 90a8d1ea318041d9adb070a834d4e0f6

name:
  - use: official
    family: Williams
    given:
      - Sarah
      - Jane

birthDate: 1992-03-20

meta:
  lastUpdated: 2026-01-23T13:58:04.099304Z
"#;

        let patient_data = Patient::parse(input).expect("parse yaml");
        let output = Patient::render(&patient_data).expect("render patient");
        let reparsed = Patient::parse(&output).expect("reparse yaml");
        assert_eq!(patient_data, reparsed);
    }

    #[test]
    fn strict_validation_rejects_unknown_keys() {
        let input = r#"resourceType: Patient
id: 90a8d1ea318041d9adb070a834d4e0f6
name:
  - use: official
    family: Williams
    given:
      - Sarah
unexpected_key: should_fail
"#;

        let err = Patient::parse(input).expect_err("should reject unknown key");
        match err {
            FhirError::Translation(msg) => {
                assert!(msg.contains("unexpected_key"));
            }
            other => panic!("expected Translation error, got {other:?}"),
        }
    }

    #[test]
    fn strict_validation_rejects_wrong_types() {
        let input = r#"resourceType: Patient
id: 90a8d1ea318041d9adb070a834d4e0f6
name:
  - use: official
    family: Williams
    given: "not_an_array"
"#;

        let err = Patient::parse(input).expect_err("should reject wrong type");
        match err {
            FhirError::Translation(msg) => {
                assert!(msg.contains("given"));
            }
            other => panic!("expected Translation error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_resource_type() {
        let input = r#"resourceType: NotPatient
id: 90a8d1ea318041d9adb070a834d4e0f6
name:
  - use: official
    family: Williams
    given:
      - Sarah
"#;

        let err = Patient::parse(input).expect_err("should reject invalid resourceType");
        match err {
            FhirError::InvalidInput(msg) => {
                assert!(msg.contains("Patient"));
                assert!(msg.contains("NotPatient"));
            }
            other => panic!("expected InvalidInput error, got {other:?}"),
        }
    }

    #[test]
    fn parses_minimal_valid_patient() {
        let input = r#"resourceType: Patient
id: 90a8d1ea318041d9adb070a834d4e0f6
"#;

        let result = Patient::parse(input).expect("should parse minimal patient");
        assert_eq!(result.id.to_string(), "90a8d1ea318041d9adb070a834d4e0f6");
        assert!(result.use_type.is_none());
        assert!(result.family.is_none());
        assert!(result.given.is_empty());
        assert!(result.birth_date.is_none());
        assert!(result.last_updated.is_none());
    }

    #[test]
    fn handles_multiple_names() {
        let input = r#"resourceType: Patient
id: 90a8d1ea318041d9adb070a834d4e0f6
name:
  - use: official
    family: Williams
    given:
      - Sarah
      - Jane
  - use: nickname
    given:
      - Sally
"#;

        let result = Patient::parse(input).expect("should parse multiple names");
        // Flat structure extracts only the first name
        assert_eq!(result.use_type, Some(NameUse::Official));
        assert_eq!(result.family, Some("Williams".to_string()));
        assert_eq!(result.given, vec!["Sarah", "Jane"]);
    }

    #[test]
    fn handles_birth_date() {
        let input = r#"resourceType: Patient
id: 90a8d1ea318041d9adb070a834d4e0f6
birthDate: 1992-03-20
"#;

        let result = Patient::parse(input).expect("should parse birth date");
        assert_eq!(result.birth_date, Some("1992-03-20".to_string()));
    }

    #[test]
    fn handles_meta() {
        let input = r#"resourceType: Patient
id: 90a8d1ea318041d9adb070a834d4e0f6
meta:
  lastUpdated: 2026-01-23T13:58:04.099304Z
"#;

        let result = Patient::parse(input).expect("should parse meta");
        assert!(result.last_updated.is_some());
        assert_eq!(
            result.last_updated.unwrap().to_rfc3339(),
            "2026-01-23T13:58:04.099304+00:00"
        );
    }

    #[test]
    fn renders_with_all_fields() {
        let id = ShardableUuid::parse("90a8d1ea318041d9adb070a834d4e0f6").expect("valid uuid");
        let last_updated = "2026-01-23T13:58:04.099304Z"
            .parse::<DateTime<Utc>>()
            .expect("valid datetime");

        let data = PatientData {
            id,
            use_type: Some(NameUse::Official),
            family: Some("Williams".to_string()),
            given: vec!["Sarah".to_string(), "Jane".to_string()],
            birth_date: Some("1992-03-20".to_string()),
            last_updated: Some(last_updated),
        };

        let yaml = Patient::render(&data).expect("should render patient");
        assert!(yaml.contains("resourceType: Patient"));
        assert!(yaml.contains("id: 90a8d1ea318041d9adb070a834d4e0f6"));
        assert!(yaml.contains("use: official"));
        assert!(yaml.contains("family: Williams"));
        assert!(yaml.contains("- Sarah"));
        assert!(yaml.contains("- Jane"));
        // YAML serializer may not quote the date string
        assert!(yaml.contains("birthDate: '1992-03-20'") || yaml.contains("birthDate: 1992-03-20"));
        // DateTime serialization uses +00:00 timezone format
        assert!(
            yaml.contains("lastUpdated: 2026-01-23T13:58:04.099304Z")
                || yaml.contains("lastUpdated: 2026-01-23T13:58:04.099304+00:00")
        );
    }

    #[test]
    fn renders_minimal_patient() {
        let id = ShardableUuid::parse("00000000000000000000000000000001").expect("valid uuid");

        let data = PatientData {
            id,
            use_type: None,
            family: None,
            given: vec![],
            birth_date: None,
            last_updated: None,
        };

        let yaml = Patient::render(&data).expect("should render minimal patient");
        assert!(yaml.contains("resourceType: Patient"));
        assert!(yaml.contains("id:"));
        // Optional fields should not be present
        assert!(!yaml.contains("name:"));
        assert!(!yaml.contains("birthDate"));
        assert!(!yaml.contains("meta:"));
    }
}
