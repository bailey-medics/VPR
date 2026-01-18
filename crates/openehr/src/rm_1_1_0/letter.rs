//! RM 1.x `COMPOSITION` (letter) wire model and translation helpers.
//!
//! This module defines the on-disk YAML representation used for an openEHR `COMPOSITION`
//! representing a clinical letter, aligned to the openEHR RM 1.x structure.
//!
//! Responsibilities:
//! - Define a strict wire model (`Letter`) for serialisation/deserialisation.
//! - Preserve YAML shapes for clinical correspondence.
//! - Provide translation helpers between domain primitives and the wire model.
//!
//! Notes:
//! - Clinical meaning lives in domain logic; this crate focuses on file formats and standards
//!   alignment.

use super::MODULE_RM_VERSION;
use crate::data_types::DvText;
use crate::OpenEhrError;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use vpr_uuid::TimestampId;

/// RM 1.x-aligned wire representation of `COMPOSITION` (letter) for on-disk YAML.
///
/// Notes:
/// - This is a wire model: it intentionally includes openEHR RM fields and types.
/// - Optional RM fields are represented as `Option<T>`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Letter {
    pub rm_version: String,
    pub uid: String,
    pub archetype_node_id: String,
    pub name: DvText,
    pub category: DvText,
    pub composer: Composer,
    pub context: Context,
    pub content: Vec<ContentItem>,
}

impl Letter {
    /// Convert this Letter to its YAML string representation.
    ///
    /// # Returns
    ///
    /// Returns a YAML string representation of this Letter.
    ///
    /// # Errors
    ///
    /// Returns [`OpenEhrError`] if serialisation fails.
    pub fn to_string(&self) -> Result<String, OpenEhrError> {
        serde_yaml::to_string(self)
            .map_err(|e| OpenEhrError::Translation(format!("Failed to serialize Letter: {}", e)))
    }
}

/// Composer information for the letter.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Composer {
    pub name: String,
    pub role: String,
}

/// Context information for the letter.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub start_time: String,
}

/// Content item wrapper (can be a section).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContentItem {
    pub section: Section,
}

/// RM `SECTION` representation.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub archetype_node_id: String,
    pub name: DvText,
    pub items: Vec<SectionItem>,
}

/// Section item wrapper (can be an evaluation).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SectionItem {
    pub evaluation: Evaluation,
}

/// RM `EVALUATION` representation.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Evaluation {
    pub archetype_node_id: String,
    pub name: DvText,
    pub data: EvaluationData,
}

/// Evaluation data wrapper.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvaluationData {
    pub narrative: Narrative,
}

/// Narrative content that can reference an external file.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Narrative {
    #[serde(rename = "type")]
    pub type_: String,
    pub path: String,
}

/// Parse an RM 1.x `COMPOSITION` (letter) from YAML text.
///
/// This uses `serde_path_to_error` to surface a best-effort "path" (e.g. `composer.name`)
/// to the failing field when the YAML does not match the `Letter` wire schema.
///
/// # Arguments
///
/// * `yaml_text` - YAML text expected to represent a `COMPOSITION` (letter) mapping.
///
/// # Returns
///
/// Returns a valid [`Letter`] on success.
///
/// # Errors
///
/// Returns [`OpenEhrError`] if:
/// - the YAML does not represent a `COMPOSITION` (letter) mapping,
/// - any field has an unexpected type,
/// - any unknown keys are present (due to `#[serde(deny_unknown_fields)]`).
pub fn composition_parse(yaml_text: &str) -> Result<Letter, OpenEhrError> {
    let deserializer = serde_yaml::Deserializer::from_str(yaml_text);

    match serde_path_to_error::deserialize(deserializer) {
        Ok(parsed) => Ok(parsed),
        Err(err) => {
            let path = err.path().to_string();
            let source = err.into_inner();
            let path = if path.is_empty() {
                "<root>"
            } else {
                path.as_str()
            };
            Err(OpenEhrError::Translation(format!(
                "Letter schema mismatch at {path}: {source}"
            )))
        }
    }
}

/// Render a `COMPOSITION` (letter) as YAML text, optionally updating specific fields.
///
/// This is a convenience function that either modifies an existing letter or creates a new one.
///
/// # Arguments
///
/// * `previous_data` - Optional YAML text expected to represent a `COMPOSITION` (letter) mapping.
///   If provided, the existing letter is parsed and modified. If None, a new letter is created.
/// * `uid` - Optional timestamp-based unique identifier.
/// * `composer_name` - Optional composer name to update.
/// * `composer_role` - Optional composer role to update.
/// * `start_time` - Optional start time to update as a UTC datetime.
///
/// # Returns
///
/// Returns YAML text representing a valid [`Letter`] on success.
///
/// # Errors
///
/// Returns [`OpenEhrError`] if:
/// - the YAML does not represent a `COMPOSITION` (letter) mapping,
/// - any field has an unexpected type,
/// - any unknown keys are present,
/// - `previous_data` is None and not enough information is provided to create a new letter.
pub fn composition_render(
    previous_data: Option<&str>,
    uid: Option<&TimestampId>,
    composer_name: Option<&str>,
    composer_role: Option<&str>,
    start_time: Option<DateTime<Utc>>,
) -> Result<String, OpenEhrError> {
    let previous_yaml = previous_data.map(composition_parse).transpose()?;

    match previous_yaml {
        Some(mut letter) => {
            // Update fields if provided
            if let Some(id) = uid {
                letter.uid = id.to_string();
            }
            if let Some(name) = composer_name {
                letter.composer.name = name.to_string();
            }
            if let Some(role) = composer_role {
                letter.composer.role = role.to_string();
            }
            if let Some(time) = start_time {
                letter.context.start_time = time.to_rfc3339_opts(SecondsFormat::Secs, true);
            }

            letter.to_string()
        }
        None => {
            // Create a new letter - uses MODULE_RM_VERSION for this module
            let version = MODULE_RM_VERSION.as_str();
            let id = uid.map(|id| id.to_string()).ok_or_else(|| {
                OpenEhrError::Translation("Cannot create Letter: uid is required".to_string())
            })?;
            let name = composer_name.ok_or_else(|| {
                OpenEhrError::Translation(
                    "Cannot create Letter: composer_name is required".to_string(),
                )
            })?;
            let role = composer_role.ok_or_else(|| {
                OpenEhrError::Translation(
                    "Cannot create Letter: composer_role is required".to_string(),
                )
            })?;
            let time = start_time.ok_or_else(|| {
                OpenEhrError::Translation(
                    "Cannot create Letter: start_time is required".to_string(),
                )
            })?;

            letter_init(version, &id, name, role, time).to_string()
        }
    }
}

/// Create a new RM 1.x `COMPOSITION` (letter) wire struct from provided values.
///
/// This creates a new Letter with default structure and provided values.
///
/// # Arguments
///
/// * `rm_version` - RM version string.
/// * `uid` - Unique identifier.
/// * `composer_name` - Composer's name.
/// * `composer_role` - Composer's role.
/// * `start_time` - Context start time as a UTC datetime.
///
/// # Returns
///
/// Returns a new [`Letter`] wire struct.
fn letter_init(
    rm_version: &str,
    uid: &str,
    composer_name: &str,
    composer_role: &str,
    start_time: DateTime<Utc>,
) -> Letter {
    Letter {
        rm_version: rm_version.to_string(),
        uid: uid.to_string(),
        archetype_node_id: "openEHR-EHR-COMPOSITION.correspondence.v1".to_string(),
        name: DvText {
            value: "Clinical letter".to_string(),
        },
        category: DvText {
            value: "event".to_string(),
        },
        composer: Composer {
            name: composer_name.to_string(),
            role: composer_role.to_string(),
        },
        context: Context {
            start_time: start_time.to_rfc3339_opts(SecondsFormat::Secs, true),
        },
        content: vec![ContentItem {
            section: Section {
                archetype_node_id: "openEHR-EHR-SECTION.correspondence.v1".to_string(),
                name: DvText {
                    value: "Correspondence".to_string(),
                },
                items: vec![SectionItem {
                    evaluation: Evaluation {
                        archetype_node_id: "openEHR-EHR-EVALUATION.clinical_correspondence.v1"
                            .to_string(),
                        name: DvText {
                            value: "Clinical correspondence".to_string(),
                        },
                        data: EvaluationData {
                            narrative: Narrative {
                                type_: "external_text".to_string(),
                                path: "./body.md".to_string(),
                            },
                        },
                    },
                }],
            },
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_sample_yaml() {
        let input = r#"rm_version: "1.0.4"
uid: "20260111T143522.045Z-550e8400e29b41d4a716446655440000"
archetype_node_id: "openEHR-EHR-COMPOSITION.correspondence.v1"
name:
  value: "Clinical letter"
category:
  value: "event"
composer:
  name: "Dr Jane Smith"
  role: "Consultant Physician"
context:
  start_time: "2026-01-12T10:14:00Z"
content:
  - section:
      archetype_node_id: "openEHR-EHR-SECTION.correspondence.v1"
      name:
        value: "Correspondence"
      items:
        - evaluation:
            archetype_node_id: "openEHR-EHR-EVALUATION.clinical_correspondence.v1"
            name:
              value: "Clinical correspondence"
            data:
              narrative:
                type: "external_text"
                path: "./body.md"
"#;

        let letter = composition_parse(input).expect("parse yaml");
        let output = serde_yaml::to_string(&letter).expect("write yaml");
        let reparsed = composition_parse(&output).expect("reparse yaml");
        assert_eq!(letter, reparsed);
    }

    #[test]
    fn strict_value_rejects_unknown_keys() {
        let input = r#"rm_version: "1.0.4"
uid: "20260111T143522.045Z-550e8400e29b41d4a716446655440000"
archetype_node_id: "openEHR-EHR-COMPOSITION.correspondence.v1"
name:
  value: "Clinical letter"
category:
  value: "event"
composer:
  name: "Dr Jane Smith"
  role: "Consultant Physician"
context:
  start_time: "2026-01-12T10:14:00Z"
content:
  - section:
      archetype_node_id: "openEHR-EHR-SECTION.correspondence.v1"
      name:
        value: "Correspondence"
      items:
        - evaluation:
            archetype_node_id: "openEHR-EHR-EVALUATION.clinical_correspondence.v1"
            name:
              value: "Clinical correspondence"
            data:
              narrative:
                type: "external_text"
                path: "./body.md"
unexpected_key: "should fail"
"#;

        let err = composition_parse(input).expect_err("should reject unknown key");
        match err {
            OpenEhrError::Translation(msg) => {
                assert!(msg.contains("unexpected_key"));
                assert!(msg.contains("unknown field") || msg.contains("unknown variant"));
            }
            other => panic!("expected Translation error, got {other:?}"),
        }
    }

    #[test]
    fn strict_value_rejects_wrong_types() {
        let wrong_type = r#"rm_version: "1.0.4"
uid: "20260111T143522.045Z-550e8400e29b41d4a716446655440000"
archetype_node_id: "openEHR-EHR-COMPOSITION.correspondence.v1"
name:
  value: "Clinical letter"
category:
  value: "event"
composer:
  name: "Dr Jane Smith"
  role: "Consultant Physician"
context:
  start_time: "2026-01-12T10:14:00Z"
content: "this should be an array"
"#;

        let err = composition_parse(wrong_type).expect_err("should reject wrong type");
        match err {
            OpenEhrError::Translation(msg) => {
                assert!(msg.contains("content"));
                assert!(msg.contains("invalid type") || msg.contains("expected"));
            }
            other => panic!("expected Translation error, got {other:?}"),
        }
    }

    #[test]
    fn letter_render_modifies_fields() {
        let yaml = r#"rm_version: "1.0.4"
uid: "20260111T143522.045Z-550e8400e29b41d4a716446655440000"
archetype_node_id: "openEHR-EHR-COMPOSITION.correspondence.v1"
name:
  value: "Clinical letter"
category:
  value: "event"
composer:
  name: "Dr Jane Smith"
  role: "Consultant Physician"
context:
  start_time: "2026-01-12T10:14:00Z"
content:
  - section:
      archetype_node_id: "openEHR-EHR-SECTION.correspondence.v1"
      name:
        value: "Correspondence"
      items:
        - evaluation:
            archetype_node_id: "openEHR-EHR-EVALUATION.clinical_correspondence.v1"
            name:
              value: "Clinical correspondence"
            data:
              narrative:
                type: "external_text"
                path: "./body.md"
"#;

        let uid = "20260113T153000.000Z-123e4567e89b12d3a456426614174000"
            .parse::<TimestampId>()
            .expect("should parse valid timestamp id");
        let start_time = DateTime::parse_from_rfc3339("2026-01-13T15:30:00Z")
            .expect("valid datetime")
            .with_timezone(&Utc);
        let result_yaml = composition_render(
            Some(yaml),
            Some(&uid),
            Some("Dr John Doe"),
            Some("Senior Consultant"),
            Some(start_time),
        )
        .expect("composition_render should work");

        let result = composition_parse(&result_yaml).expect("should parse returned YAML");

        assert_eq!(result.rm_version, "1.0.4");
        assert_eq!(
            result.uid,
            "20260113T153000.000Z-123e4567-e89b-12d3-a456-426614174000"
        );
        assert_eq!(result.composer.name, "Dr John Doe");
        assert_eq!(result.composer.role, "Senior Consultant");
        assert_eq!(result.context.start_time, "2026-01-13T15:30:00Z");
    }

    #[test]
    fn letter_render_partial_update() {
        let yaml = r#"rm_version: "1.0.4"
uid: "20260111T143522.045Z-550e8400e29b41d4a716446655440000"
archetype_node_id: "openEHR-EHR-COMPOSITION.correspondence.v1"
name:
  value: "Clinical letter"
category:
  value: "event"
composer:
  name: "Dr Jane Smith"
  role: "Consultant Physician"
context:
  start_time: "2026-01-12T10:14:00Z"
content:
  - section:
      archetype_node_id: "openEHR-EHR-SECTION.correspondence.v1"
      name:
        value: "Correspondence"
      items:
        - evaluation:
            archetype_node_id: "openEHR-EHR-EVALUATION.clinical_correspondence.v1"
            name:
              value: "Clinical correspondence"
            data:
              narrative:
                type: "external_text"
                path: "./body.md"
"#;

        let result_yaml = composition_render(Some(yaml), None, Some("Dr Updated Name"), None, None)
            .expect("composition_render should work with partial update");

        let result = composition_parse(&result_yaml).expect("should parse returned YAML");

        // Only composer name should be updated
        assert_eq!(result.rm_version, "1.0.4");
        assert_eq!(
            result.uid,
            "20260111T143522.045Z-550e8400e29b41d4a716446655440000"
        );
        assert_eq!(result.composer.name, "Dr Updated Name");
        assert_eq!(result.composer.role, "Consultant Physician");
        assert_eq!(result.context.start_time, "2026-01-12T10:14:00Z");
    }

    #[test]
    fn letter_init_creates_valid_structure() {
        let start_time = DateTime::parse_from_rfc3339("2026-01-12T00:00:00Z")
            .expect("valid datetime")
            .with_timezone(&Utc);
        let letter = letter_init("1.0.4", "test-uid", "Dr Test", "Test Role", start_time);

        assert_eq!(letter.rm_version, "1.0.4");
        assert_eq!(letter.uid, "test-uid");
        assert_eq!(
            letter.archetype_node_id,
            "openEHR-EHR-COMPOSITION.correspondence.v1"
        );
        assert_eq!(letter.name.value, "Clinical letter");
        assert_eq!(letter.category.value, "event");
        assert_eq!(letter.composer.name, "Dr Test");
        assert_eq!(letter.composer.role, "Test Role");
        assert_eq!(letter.context.start_time, "2026-01-12T00:00:00Z");
        assert_eq!(letter.content.len(), 1);
        assert_eq!(
            letter.content[0].section.archetype_node_id,
            "openEHR-EHR-SECTION.correspondence.v1"
        );
    }

    #[test]
    fn letter_to_string_works() {
        let start_time = DateTime::parse_from_rfc3339("2026-01-12T00:00:00Z")
            .expect("valid datetime")
            .with_timezone(&Utc);
        let letter = letter_init("1.0.4", "test-uid", "Dr Test", "Test Role", start_time);

        let yaml_string = letter.to_string().expect("to_string should work");

        assert!(yaml_string.contains("rm_version:"));
        assert!(yaml_string.contains("uid: test-uid"));
        assert!(yaml_string.contains("composer:"));
        assert!(yaml_string.contains("name: Dr Test"));
        assert!(yaml_string.contains("role: Test Role"));
        assert!(yaml_string.contains("start_time:"));

        // Verify it can be parsed back
        let reparsed = composition_parse(&yaml_string).expect("should parse the generated YAML");
        assert_eq!(reparsed, letter);
    }

    #[test]
    fn letter_render_rejects_create_without_required_fields() {
        // Test that required fields are still checked (uid is first required field)
        let start_time = DateTime::parse_from_rfc3339("2026-01-18T00:00:00Z")
            .expect("valid datetime")
            .with_timezone(&Utc);
        let err = composition_render(None, None, Some("Dr Test"), Some("Role"), Some(start_time))
            .expect_err("should reject when creating without uid");

        match err {
            OpenEhrError::Translation(msg) => {
                assert!(msg.contains("uid is required"));
            }
            other => panic!("expected Translation error, got {other:?}"),
        }
    }

    #[test]
    fn letter_render_creates_new_from_scratch() {
        let uid = "20260112T100000.000Z-00000000000000000000000000000000"
            .parse::<TimestampId>()
            .expect("should parse valid timestamp id");
        let start_time = DateTime::parse_from_rfc3339("2026-01-12T10:00:00Z")
            .expect("valid datetime")
            .with_timezone(&Utc);
        let result_yaml = composition_render(
            None,
            Some(&uid),
            Some("Dr New"),
            Some("New Role"),
            Some(start_time),
        )
        .expect("composition_render should create new letter");

        let result = composition_parse(&result_yaml).expect("should parse the result");

        assert_eq!(result.rm_version, "rm_1_1_0");
        assert_eq!(
            result.uid,
            "20260112T100000.000Z-00000000-0000-0000-0000-000000000000"
        );
        assert_eq!(result.composer.name, "Dr New");
        assert_eq!(result.composer.role, "New Role");
        assert_eq!(result.context.start_time, "2026-01-12T10:00:00Z");
        assert_eq!(
            result.archetype_node_id,
            "openEHR-EHR-COMPOSITION.correspondence.v1"
        );
        assert_eq!(result.name.value, "Clinical letter");
        assert_eq!(result.category.value, "event");
    }
}
