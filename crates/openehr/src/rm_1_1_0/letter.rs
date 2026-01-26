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

use crate::data_types::{ArchetypeId, DvText};
use crate::public_structs::letter::ClinicalList as PublicClinicalList;
use crate::{LetterData, OpenEhrError, RmVersion, TimestampId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vpr_types::NonEmptyText;

/// RM 1.x-aligned wire representation of `COMPOSITION` (letter) for on-disk YAML.
///
/// Notes:
/// - This is a wire model: it intentionally includes openEHR RM fields and types.
/// - Optional RM fields are represented as `Option<T>`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Composition {
    pub rm_version: String,
    pub uid: String,
    pub archetype_node_id: String,
    pub name: DvText,
    pub category: DvText,
    pub composer: Composer,
    pub context: Context,
    pub content: Vec<ContentItem>,
}

/// Returns the archetype node ID for the letter Composition.
fn archetype_node_id() -> ArchetypeId {
    ArchetypeId::parse("openEHR-EHR-COMPOSITION.correspondence.v1")
        .expect("composition archetype ID is valid")
}

/// Default name for the letter Composition data.
const NAME: &str = "Clinical letter";

/// Default category for the letter Composition data.
const CATEGORY: &str = "event";

/// Composer information for the letter.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Composer {
    pub name: String,
    pub role: String,
}

/// Context information for the letter.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Context {
    pub start_time: DateTime<Utc>,
}

/// Content item wrapper (can be a section).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ContentItem {
    pub section: Section,
}

/// RM `SECTION` representation.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Section {
    pub archetype_node_id: String,
    pub name: DvText,
    pub items: Vec<SectionItem>,
}

/// Returns the archetype node ID for SECTION (correspondence).
fn section_archetype_node_id() -> ArchetypeId {
    ArchetypeId::parse("openEHR-EHR-SECTION.correspondence.v1")
        .expect("section archetype ID is valid")
}

/// Default name for SECTION (correspondence).
const SECTION_NAME: &str = "Correspondence";

/// Section item wrapper (can be an evaluation).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SectionItem {
    pub evaluation: Evaluation,
}

/// RM `EVALUATION` representation.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Evaluation {
    pub archetype_node_id: String,
    pub name: DvText,
    pub data: EvaluationData,
}

/// Returns the archetype node ID for Evaluation (clinical correspondence).
fn evaluation_archetype_node_id() -> ArchetypeId {
    ArchetypeId::parse("openEHR-EHR-EVALUATION.clinical_correspondence.v1")
        .expect("evaluation archetype ID is valid")
}

/// Default name for Evaluation (clinical correspondence).
const EVALUATION_NAME: &str = "Clinical correspondence";

/// Evaluation data wrapper.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
enum EvaluationData {
    Narrative {
        narrative: Narrative,
    },
    ClinicalList {
        kind: DvText,
        items: Vec<ClinicalListItem>,
    },
}

/// Narrative content that can reference an external file.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Narrative {
    #[serde(rename = "type")]
    pub type_: String,
    pub path: String,
}

/// Default type for external text narrative (body.md).
const NARRATIVE_TYPE_TEXT: &str = "external_text";

/// Type for external media narrative (attachments).
const NARRATIVE_TYPE_MEDIA: &str = "external_media";

/// Default path for narrative body.
const NARRATIVE_PATH: &str = "./body.md";

/// Content parameters for letter initialization.
struct LetterContent<'a> {
    clinical_lists: Option<&'a [PublicClinicalList]>,
    has_body: bool,
    attachments: &'a [crate::public_structs::letter::AttachmentReference],
}

/// RM-specific ClinicalList EVALUATION (maps internally from public ClinicalList).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ClinicalList {
    archetype_node_id: String,
    name: DvText,
    data: ClinicalListData,
}

/// Returns the archetype node ID for ClinicalList EVALUATION.
/// OpenEHR calls these "snapshot" evaluations.
fn snapshot_archetype_node_id() -> ArchetypeId {
    ArchetypeId::parse("openEHR-EHR-EVALUATION.snapshot.v1")
        .expect("snapshot archetype ID is valid")
}

/// ClinicalList data structure.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ClinicalListData {
    kind: DvText,
    items: Vec<ClinicalListItem>,
}

/// A single item within a ClinicalList.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ClinicalListItem {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<Code>,
}

/// A coded concept with terminology and value.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Code {
    pub terminology: String,
    pub value: String,
}

/// Converts a public domain-level clinical list to wire format.
///
/// This implementation transforms the domain type [`PublicClinicalList`] into the
/// internal wire representation [`ClinicalList`] used for YAML serialization.
///
/// The conversion:
/// - Wraps the list name in a [`DvText`] structure required by openEHR RM
/// - Assigns the snapshot evaluation archetype ID (`openEHR-EHR-EVALUATION.snapshot.v1`)
/// - Wraps the kind field in [`DvText`]
/// - Converts each item, preserving text and optional coded concepts
/// - Maps [`CodedConcept`] to wire format [`Code`] structures
///
/// # Examples
///
/// ```rust,ignore
/// use openehr::ClinicalList as PublicClinicalList;
///
/// let public_list = PublicClinicalList {
///     name: "Diagnoses (snapshot)".to_string(),
///     kind: "diagnoses".to_string(),
///     items: vec![],
/// };
///
/// let wire_list: ClinicalList = (&public_list).into();
/// ```
impl ClinicalList {
    fn try_from_public(list: &PublicClinicalList) -> Result<Self, OpenEhrError> {
        Ok(ClinicalList {
            archetype_node_id: snapshot_archetype_node_id().to_string(),
            name: DvText {
                value: NonEmptyText::new(&list.name).map_err(|_| {
                    OpenEhrError::InvalidInput("list name cannot be empty".to_string())
                })?,
            },
            data: ClinicalListData {
                kind: DvText {
                    value: NonEmptyText::new(&list.kind).map_err(|_| {
                        OpenEhrError::InvalidInput("list kind cannot be empty".to_string())
                    })?,
                },
                items: list
                    .items
                    .iter()
                    .map(|item| ClinicalListItem {
                        text: item.text.clone(),
                        code: item.code.as_ref().map(|c| Code {
                            terminology: c.terminology.clone(),
                            value: c.value.clone(),
                        }),
                    })
                    .collect(),
            },
        })
    }
}

/// Extracts domain-level letter data from wire format composition.
///
/// This implementation parses a deserialized [`Composition`] (wire format) and extracts
/// the essential domain-level fields into a [`LetterData`] carrier struct.
///
/// The extraction process:
/// 1. **Parses rm_version**: Converts the string to [`RmVersion`] enum, defaulting to
///    `rm_1_1_0` if parsing fails
/// 2. **Extracts scalar fields**: uid, composer name/role, start_time
/// 3. **Walks nested structure**: Traverses content → section → items to find evaluations
/// 4. **Filters clinical lists**: Only extracts [`EvaluationData::ClinicalList`] variants,
///    ignoring narrative evaluations
/// 5. **Converts to public types**: Maps wire format clinical lists and coded concepts
///    to public domain types
///
/// This conversion is used by [`composition_parse()`] to provide a clean domain-level
/// API that hides the complexity of the openEHR RM structure.
///
/// # Panics
///
/// Does not panic. Invalid rm_version strings fall back to `rm_1_1_0`.
///
/// # Examples
///
/// ```rust,ignore
/// let composition: Composition = serde_yaml::from_str(yaml)?;
/// let letter_data: LetterData = composition.into();
///
/// assert_eq!(letter_data.composer_name, "Dr Jane Smith");
/// assert_eq!(letter_data.clinical_lists.len(), 2);
/// ```
impl From<Composition> for LetterData {
    fn from(comp: Composition) -> Self {
        // Extract clinical lists, attachments, and check for body from the composition content
        let mut clinical_lists = Vec::new();
        let mut attachments = Vec::new();
        let mut has_body = false;

        for item in &comp.content {
            for section_item in &item.section.items {
                match &section_item.evaluation.data {
                    EvaluationData::ClinicalList { kind, items } => {
                        clinical_lists.push(PublicClinicalList {
                            name: section_item.evaluation.name.value.to_string(),
                            kind: kind.value.to_string(),
                            items: items
                                .iter()
                                .map(|item| crate::public_structs::letter::ClinicalListItem {
                                    text: item.text.clone(),
                                    code: item.code.as_ref().map(|c| {
                                        crate::public_structs::letter::CodedConcept {
                                            terminology: c.terminology.clone(),
                                            value: c.value.clone(),
                                        }
                                    }),
                                })
                                .collect(),
                        });
                    }
                    EvaluationData::Narrative { narrative } => {
                        if narrative.type_ == NARRATIVE_TYPE_MEDIA {
                            // external_media = attachment
                            attachments.push(crate::public_structs::letter::AttachmentReference {
                                path: narrative.path.clone(),
                            });
                        } else if narrative.type_ == NARRATIVE_TYPE_TEXT
                            && narrative.path == NARRATIVE_PATH
                        {
                            // external_text pointing to body.md
                            has_body = true;
                        }
                    }
                }
            }
        }

        LetterData {
            rm_version: comp
                .rm_version
                .parse::<RmVersion>()
                .unwrap_or(RmVersion::rm_1_1_0),
            uid: comp
                .uid
                .parse()
                .unwrap_or_else(|_| TimestampId::new(Utc::now(), Uuid::new_v4())),
            composer_name: comp.composer.name,
            composer_role: comp.composer.role,
            start_time: comp.context.start_time,
            clinical_lists,
            has_body,
            attachments,
        }
    }
}

/// Builds wire format composition from domain-level letter data.
///
/// This implementation creates a complete openEHR RM [`Composition`] structure from
/// the simplified domain-level [`LetterData`] by delegating to the internal
/// `letter_init()` function.
///
/// The conversion:
/// - Constructs a full openEHR RM COMPOSITION with proper archetype IDs
/// - Creates the correspondence SECTION with required structure
/// - Adds narrative EVALUATIONs based on attachments or default body.md
/// - Converts clinical lists to snapshot EVALUATIONs with proper archetype IDs
/// - Wraps all text fields in [`DvText`] structures as required by the RM
///
/// This is used by [`composition_render()`] to transform domain data into
/// serializable YAML that conforms to the openEHR specification.
///
/// # Examples
///
/// ```rust,ignore
/// let letter_data = LetterData {
///     rm_version: RmVersion::rm_1_1_0,
///     uid: "20260112T100000.000Z-uuid".to_string(),
///     composer_name: "Dr Smith".to_string(),
///     composer_role: "Consultant".to_string(),
///     start_time: Utc::now(),
///     clinical_lists: vec![],
///     attachments: vec![],
/// };
///
/// let composition: Composition = (&letter_data).into();
/// let yaml = serde_yaml::to_string(&composition)?;
/// ```
impl From<&LetterData> for Composition {
    fn from(data: &LetterData) -> Self {
        letter_init(
            data.rm_version.as_str(),
            &data.uid.to_string(),
            &data.composer_name,
            &data.composer_role,
            data.start_time,
            LetterContent {
                clinical_lists: Some(&data.clinical_lists),
                has_body: data.has_body,
                attachments: &data.attachments,
            },
        )
        .expect("letter_init should not fail with valid LetterData")
    }
}

/// Parse an RM 1.x `COMPOSITION` (letter) from YAML text.
///
/// This uses `serde_path_to_error` to surface a best-effort "path" (e.g. `composer.name`)
/// to the failing field when the YAML does not match the `Composition` wire schema.
///
/// # Arguments
///
/// * `yaml_text` - YAML text expected to represent a `COMPOSITION` (letter) mapping.
///
/// # Returns
///
/// Returns a [`LetterData`] with domain-level fields extracted from the composition.
///
/// # Errors
///
/// Returns [`OpenEhrError`] if:
/// - the YAML does not represent a `COMPOSITION` (letter) mapping,
/// - any field has an unexpected type,
/// - any unknown keys are present (due to `#[serde(deny_unknown_fields)]`).
pub fn composition_parse(yaml_text: &str) -> Result<LetterData, OpenEhrError> {
    let deserializer = serde_yaml::Deserializer::from_str(yaml_text);

    match serde_path_to_error::deserialize::<_, Composition>(deserializer) {
        Ok(parsed) => Ok(LetterData::from(parsed)),
        Err(err) => {
            let path = err.path().to_string();
            let source = err.into_inner();
            let path = if path.is_empty() {
                "<root>"
            } else {
                path.as_str()
            };
            Err(OpenEhrError::Translation(format!(
                "Composition schema mismatch at {path}: {source}"
            )))
        }
    }
}

/// Render a `COMPOSITION` (letter) as YAML text from letter data.
///
/// This converts domain-level [`LetterData`] into wire format and serializes to YAML.
///
/// # Arguments
///
/// * `data` - Letter data containing all composition fields.
///
/// # Returns
///
/// Returns YAML text representing a valid [`Composition`] on success.
///
/// # Errors
///
/// Returns [`OpenEhrError`] if serialization fails.
pub fn composition_render(data: &LetterData) -> Result<String, OpenEhrError> {
    let composition: Composition = data.into();
    serde_yaml::to_string(&composition)
        .map_err(|e| OpenEhrError::Translation(format!("Failed to serialize composition: {e}")))
}

/// Create a new RM 1.x `COMPOSITION` (letter) wire struct from provided values.
///
/// This creates a new Composition with default structure and provided values.
///
/// # Arguments
///
/// * `rm_version` - RM version string.
/// * `uid` - Unique identifier.
/// * `composer_name` - Composer's name.
/// * `composer_role` - Composer's role.
/// * `start_time` - Context start time as a UTC datetime.
/// * `content` - Letter content including clinical lists, body flag, and attachments.
///
/// # Returns
///
/// Returns a new [`Composition`] wire struct.
///
/// # Errors
///
/// Returns [`OpenEhrError`] if creating NonEmptyText values fails.
fn letter_init(
    rm_version: &str,
    uid: &str,
    composer_name: &str,
    composer_role: &str,
    start_time: DateTime<Utc>,
    content: LetterContent<'_>,
) -> Result<Composition, OpenEhrError> {
    let mut section_items = Vec::new();

    // Add body.md narrative if present
    if content.has_body {
        section_items.push(SectionItem {
            evaluation: Evaluation {
                archetype_node_id: evaluation_archetype_node_id().to_string(),
                name: DvText {
                    value: NonEmptyText::new(EVALUATION_NAME)
                        .expect("EVALUATION_NAME is non-empty"),
                },
                data: EvaluationData::Narrative {
                    narrative: Narrative {
                        type_: NARRATIVE_TYPE_TEXT.to_string(),
                        path: NARRATIVE_PATH.to_string(),
                    },
                },
            },
        });
    }

    // Add attachment narratives
    for attachment in content.attachments {
        section_items.push(SectionItem {
            evaluation: Evaluation {
                archetype_node_id: evaluation_archetype_node_id().to_string(),
                name: DvText {
                    value: NonEmptyText::new(EVALUATION_NAME)
                        .expect("EVALUATION_NAME is non-empty"),
                },
                data: EvaluationData::Narrative {
                    narrative: Narrative {
                        type_: NARRATIVE_TYPE_MEDIA.to_string(),
                        path: attachment.path.clone(),
                    },
                },
            },
        });
    }

    // Add snapshot evaluations if provided
    if let Some(lists) = content.clinical_lists {
        for list in lists {
            let snapshot = ClinicalList::try_from_public(list)
                .map_err(|e| OpenEhrError::InvalidInput(e.to_string()))?;
            section_items.push(SectionItem {
                evaluation: Evaluation {
                    archetype_node_id: snapshot.archetype_node_id,
                    name: snapshot.name,
                    data: EvaluationData::ClinicalList {
                        kind: snapshot.data.kind,
                        items: snapshot.data.items,
                    },
                },
            });
        }
    }

    Ok(Composition {
        rm_version: rm_version.to_string(),
        uid: uid.to_string(),
        archetype_node_id: archetype_node_id().to_string(),
        name: DvText {
            value: NonEmptyText::new(NAME).expect("NAME is non-empty"),
        },
        category: DvText {
            value: NonEmptyText::new(CATEGORY).expect("CATEGORY is non-empty"),
        },
        composer: Composer {
            name: composer_name.to_string(),
            role: composer_role.to_string(),
        },
        context: Context { start_time },
        content: vec![ContentItem {
            section: Section {
                archetype_node_id: section_archetype_node_id().to_string(),
                name: DvText {
                    value: NonEmptyText::new(SECTION_NAME).expect("SECTION_NAME is non-empty"),
                },
                items: section_items,
            },
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_sample_yaml() {
        let input = r#"rm_version: "rm_1_1_0"
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
        let output = composition_render(&letter).expect("render letter");
        let reparsed = composition_parse(&output).expect("reparse yaml");
        assert_eq!(letter, reparsed);
    }

    #[test]
    fn strict_value_rejects_unknown_keys() {
        let input = r#"rm_version: "rm_1_1_0"
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
        let wrong_type = r#"rm_version: "rm_1_1_0"
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
        let yaml = r#"rm_version: "rm_1_1_0"
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

        // Parse the existing letter
        let mut letter_data = composition_parse(yaml).expect("should parse YAML");

        // Modify fields
        letter_data.uid = "20260113T153000.000Z-123e4567-e89b-12d3-a456-426614174000"
            .parse()
            .expect("valid TimestampId");
        letter_data.composer_name = "Dr John Doe".to_string();
        letter_data.composer_role = "Senior Consultant".to_string();
        let start_time = DateTime::parse_from_rfc3339("2026-01-13T15:30:00Z")
            .expect("valid datetime")
            .with_timezone(&Utc);
        letter_data.start_time = start_time;

        // Render it back
        let result_yaml = composition_render(&letter_data).expect("composition_render should work");

        // Parse to verify changes
        let result = composition_parse(&result_yaml).expect("should parse returned YAML");

        assert_eq!(result.rm_version.as_str(), "rm_1_1_0");
        assert_eq!(
            result.uid.to_string(),
            "20260113T153000.000Z-123e4567-e89b-12d3-a456-426614174000"
        );
        assert_eq!(result.composer_name, "Dr John Doe");
        assert_eq!(result.composer_role, "Senior Consultant");
        assert_eq!(result.start_time, start_time);
    }

    #[test]
    fn letter_render_partial_update() {
        let yaml = r#"rm_version: "rm_1_1_0"
uid: "20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000"
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

        // Parse the existing letter
        let mut letter_data = composition_parse(yaml).expect("should parse YAML");

        // Only update composer name
        letter_data.composer_name = "Dr Updated Name".to_string();

        // Render it back
        let result_yaml = composition_render(&letter_data)
            .expect("composition_render should work with partial update");

        // Parse to verify changes
        let result = composition_parse(&result_yaml).expect("should parse returned YAML");

        // Only composer name should be updated
        assert_eq!(result.rm_version.as_str(), "rm_1_1_0");
        assert_eq!(
            result.uid.to_string(),
            "20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(result.composer_name, "Dr Updated Name");
        assert_eq!(result.composer_role, "Consultant Physician");
        assert_eq!(
            result.start_time,
            DateTime::parse_from_rfc3339("2026-01-12T10:14:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
    }

    #[test]
    fn letter_init_creates_valid_structure() {
        let start_time = DateTime::parse_from_rfc3339("2026-01-12T00:00:00Z")
            .expect("valid datetime")
            .with_timezone(&Utc);
        let letter = letter_init(
            "rm_1_1_0",
            "test-uid",
            "Dr Test",
            "Test Role",
            start_time,
            LetterContent {
                clinical_lists: None,
                has_body: true,
                attachments: &[],
            },
        )
        .expect("letter_init should succeed");

        assert_eq!(letter.rm_version, "rm_1_1_0");
        assert_eq!(letter.uid, "test-uid");
        assert_eq!(letter.archetype_node_id, archetype_node_id().to_string());
        assert_eq!(letter.name.value.as_str(), NAME);
        assert_eq!(letter.category.value.as_str(), CATEGORY);
        assert_eq!(letter.composer.name, "Dr Test");
        assert_eq!(letter.composer.role, "Test Role");
        assert_eq!(letter.context.start_time, start_time);
        assert_eq!(letter.content.len(), 1);
        assert_eq!(
            letter.content[0].section.archetype_node_id,
            section_archetype_node_id().to_string()
        );
    }

    #[test]
    fn letter_to_string_works() {
        let start_time = DateTime::parse_from_rfc3339("2026-01-12T00:00:00Z")
            .expect("valid datetime")
            .with_timezone(&Utc);
        let test_uid = "20260112T000000.000Z-00000000-0000-0000-0000-000000000000";
        let letter = letter_init(
            "rm_1_1_0",
            test_uid,
            "Dr Test",
            "Test Role",
            start_time,
            LetterContent {
                clinical_lists: None,
                has_body: true,
                attachments: &[],
            },
        )
        .expect("letter_init should succeed");

        let yaml_string = serde_yaml::to_string(&letter).expect("to_string should work");

        assert!(yaml_string.contains("rm_version:"));
        assert!(yaml_string.contains(&format!("uid: {}", test_uid)));
        assert!(yaml_string.contains("composer:"));
        assert!(yaml_string.contains("name: Dr Test"));
        assert!(yaml_string.contains("role: Test Role"));
        assert!(yaml_string.contains("start_time:"));

        // Verify it can be parsed back
        let reparsed_data =
            composition_parse(&yaml_string).expect("should parse the generated YAML");
        let reparsed = Composition::from(&reparsed_data);
        assert_eq!(reparsed, letter);
    }

    #[test]
    fn letter_render_creates_new_from_scratch() {
        let start_time = DateTime::parse_from_rfc3339("2026-01-12T10:00:00Z")
            .expect("valid datetime")
            .with_timezone(&Utc);

        // Create LetterData from scratch
        let letter_data = LetterData {
            rm_version: RmVersion::rm_1_1_0,
            uid: "20260112T100000.000Z-00000000-0000-0000-0000-000000000000"
                .parse()
                .expect("valid TimestampId"),
            composer_name: "Dr New".to_string(),
            composer_role: "New Role".to_string(),
            start_time,
            clinical_lists: vec![],
            has_body: true,
            attachments: vec![],
        };

        let result_yaml =
            composition_render(&letter_data).expect("composition_render should create new letter");

        let result = composition_parse(&result_yaml).expect("should parse the result");

        assert_eq!(result.rm_version, RmVersion::rm_1_1_0);
        assert_eq!(
            result.uid.to_string(),
            "20260112T100000.000Z-00000000-0000-0000-0000-000000000000"
        );
        assert_eq!(result.composer_name, "Dr New");
        assert_eq!(result.composer_role, "New Role");
        assert_eq!(result.start_time, start_time);
    }

    #[test]
    fn letter_with_attachments_creates_external_media() {
        let start_time = DateTime::parse_from_rfc3339("2026-01-12T10:00:00Z")
            .expect("valid datetime")
            .with_timezone(&Utc);

        // Create LetterData with attachments
        let letter_data = LetterData {
            rm_version: RmVersion::rm_1_1_0,
            uid: "20260112T100000.000Z-00000000-0000-0000-0000-000000000000"
                .parse()
                .expect("valid TimestampId"),
            composer_name: "Dr Test".to_string(),
            composer_role: "Test Role".to_string(),
            start_time,
            clinical_lists: vec![],
            has_body: false,
            attachments: vec![
                crate::public_structs::letter::AttachmentReference {
                    path: "./attachments/attachment_1.yaml".to_string(),
                },
                crate::public_structs::letter::AttachmentReference {
                    path: "./attachments/attachment_2.yaml".to_string(),
                },
            ],
        };

        let yaml_string = composition_render(&letter_data).expect("composition_render should work");

        // Verify it contains external_media references
        assert!(
            yaml_string.contains("type: external_media"),
            "YAML should contain external_media type"
        );
        assert!(
            yaml_string.contains("path: ./attachments/attachment_1.yaml"),
            "YAML should contain first attachment path"
        );
        assert!(
            yaml_string.contains("path: ./attachments/attachment_2.yaml"),
            "YAML should contain second attachment path"
        );

        // Verify it does NOT contain external_text or body.md
        assert!(
            !yaml_string.contains("type: external_text"),
            "YAML should not contain external_text type when attachments are present"
        );
        assert!(
            !yaml_string.contains("path: ./body.md"),
            "YAML should not contain body.md path when attachments are present"
        );

        // Verify it can be parsed back
        let reparsed_data =
            composition_parse(&yaml_string).expect("should parse the generated YAML");

        // Check that attachments were correctly extracted
        assert_eq!(reparsed_data.attachments.len(), 2);
        assert_eq!(
            reparsed_data.attachments[0].path,
            "./attachments/attachment_1.yaml"
        );
        assert_eq!(
            reparsed_data.attachments[1].path,
            "./attachments/attachment_2.yaml"
        );
    }

    #[test]
    fn letter_with_both_body_and_attachments() {
        let start_time = DateTime::parse_from_rfc3339("2026-01-12T10:00:00Z")
            .expect("valid datetime")
            .with_timezone(&Utc);

        // Create LetterData with both body and attachments
        let letter_data = LetterData {
            rm_version: RmVersion::rm_1_1_0,
            uid: "20260112T100000.000Z-00000000-0000-0000-0000-000000000000"
                .parse()
                .expect("valid TimestampId"),
            composer_name: "Dr Test".to_string(),
            composer_role: "Test Role".to_string(),
            start_time,
            clinical_lists: vec![],
            has_body: true,
            attachments: vec![crate::public_structs::letter::AttachmentReference {
                path: "./attachments/attachment_1.yaml".to_string(),
            }],
        };

        let yaml_string = composition_render(&letter_data).expect("composition_render should work");

        // Verify it contains BOTH external_text AND external_media
        assert!(
            yaml_string.contains("type: external_text"),
            "YAML should contain external_text type for body.md"
        );
        assert!(
            yaml_string.contains("path: ./body.md"),
            "YAML should contain body.md path"
        );
        assert!(
            yaml_string.contains("type: external_media"),
            "YAML should contain external_media type for attachments"
        );
        assert!(
            yaml_string.contains("path: ./attachments/attachment_1.yaml"),
            "YAML should contain attachment path"
        );

        // Verify it can be parsed back
        let reparsed_data =
            composition_parse(&yaml_string).expect("should parse the generated YAML");

        // Check that both body and attachments were correctly extracted
        assert!(reparsed_data.has_body);
        assert_eq!(reparsed_data.attachments.len(), 1);
        assert_eq!(
            reparsed_data.attachments[0].path,
            "./attachments/attachment_1.yaml"
        );
    }
}
