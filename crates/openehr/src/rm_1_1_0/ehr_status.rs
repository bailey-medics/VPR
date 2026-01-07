//! RM 1.1.0 `EHR_STATUS` wire model and translation helpers.
//!
//! This module defines the on-disk YAML representation used for an openEHR `EHR_STATUS`
//! component, aligned to the openEHR RM 1.x structure.
//!
//! Responsibilities:
//! - Define a strict wire model (`EhrStatus`) for serialisation/deserialisation.
//! - Preserve legacy YAML shapes where `subject.external_ref` may be absent, a single object,
//!   or a list.
//! - Provide translation helpers between domain primitives and the wire model.
//!
//! Notes:
//! - Clinical meaning lives in domain logic; this crate focuses on file formats and standards
//!   alignment.

use crate::{EhrId, ExternalReference, OpenEhrError, RmVersion};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::constants::{DEFAULT_ARCHETYPE_NODE_ID, DEFAULT_EXTERNAL_REF_TYPE, DEFAULT_NAME};
use super::CURRENT_RM_VERSION;

/// RM 1.x-aligned wire representation of `EHR_STATUS` for on-disk YAML.
///
/// Notes:
/// - This is a wire model: it intentionally includes openEHR RM fields and types.
/// - Optional RM fields are represented as `Option<T>`.
/// - An `ehr_id` wrapper is persisted at the top-level of this YAML, matching the on-disk
///   component layout.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EhrStatus {
    pub rm_version: RmVersion,
    pub ehr_id: HierObjectId,
    pub archetype_node_id: String,
    pub name: DvText,
    #[serde(default, skip_serializing_if = "PartySelf::is_empty")]
    pub subject: PartySelf,
    pub is_queryable: bool,
    pub is_modifiable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other_details: Option<ItemStructure>,
}

impl EhrStatus {
    /// Convert this EhrStatus to its YAML string representation.
    ///
    /// # Returns
    ///
    /// Returns a YAML string representation of this EhrStatus.
    ///
    /// # Errors
    ///
    /// Returns [`OpenEhrError`] if serialisation fails.
    pub fn to_string(&self) -> Result<String, OpenEhrError> {
        serde_yaml::to_string(self).map_err(|e| {
            OpenEhrError::Translation(format!("Failed to serialize EHR_STATUS: {}", e))
        })
    }
}

/// RM `HIER_OBJECT_ID` (simplified to a `value` string wrapper).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HierObjectId {
    pub value: String,
}

/// RM `DV_TEXT` (simplified to a `value` string wrapper).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DvText {
    pub value: String,
}

/// RM `PARTY_SELF`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PartySelf {
    #[serde(default, skip_serializing_if = "ExternalRefs::is_empty")]
    pub external_ref: ExternalRefs,
}

impl PartySelf {
    /// Returns true if this PartySelf has no external references.
    ///
    /// Used by serde's `skip_serializing_if` to omit empty `subject` fields in YAML.
    fn is_empty(&self) -> bool {
        self.external_ref.is_empty()
    }
}

/// `subject.external_ref` may be absent, a single object, or a list.
///
/// This supports both forms to preserve compatibility with existing on-disk YAML.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExternalRefs(pub Vec<PartyRef>);

impl ExternalRefs {
    /// Returns true if this ExternalRefs contains no PartyRef entries.
    ///
    /// Used by serde's `skip_serializing_if` to omit empty `external_ref` fields in YAML.
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Internal enum for deserializing `subject.external_ref` as either a single `PartyRef` or a list.
///
/// This supports flexible YAML input where `external_ref` can be absent, a single object, or an array.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
enum OneOrManyPartyRef {
    One(PartyRef),
    Many(Vec<PartyRef>),
}

/// Custom deserializer for `ExternalRefs` to handle flexible YAML input.
///
/// Supports `external_ref` being absent (None), a single `PartyRef` object, or a list of `PartyRef`s.
impl<'de> Deserialize<'de> for ExternalRefs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<OneOrManyPartyRef>::deserialize(deserializer)?;
        let refs = match value {
            None => Vec::new(),
            Some(OneOrManyPartyRef::One(r)) => vec![r],
            Some(OneOrManyPartyRef::Many(rs)) => rs,
        };
        Ok(Self(refs))
    }
}

/// Custom serializer for `ExternalRefs` to handle flexible YAML output.
///
/// Serializes as `null` if empty, a single `PartyRef` object if one entry, or a list if multiple.
impl Serialize for ExternalRefs {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0.len() {
            0 => serializer.serialize_none(),
            1 => OneOrManyPartyRef::One(self.0[0].clone()).serialize(serializer),
            _ => OneOrManyPartyRef::Many(self.0.clone()).serialize(serializer),
        }
    }
}

/// RM `PARTY_REF` (simplified).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PartyRef {
    pub id: ObjectId,
    pub namespace: String,
    #[serde(rename = "type")]
    pub type_: String,
}

/// RM `HIER_OBJECT_ID` (simplified to a `value` string wrapper).per).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ObjectId {
    pub value: String,
}

/// RM `ITEM_STRUCTURE` (highly constrained to the needs of YAML representation).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ItemStructure {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Element>,
}

/// RM `ELEMENT` (constrained to `DV_TEXT` name/value for YAML representation).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Element {
    pub name: DvText,
    pub value: DvText,
}

/// Parse an RM 1.1.0 `EHR_STATUS` from YAML text and validate against expected domain values.
///
/// This is a convenience function that calls `ehr_status_parse_full` and validates that the
/// parsed EHR_STATUS contains the expected `ehr_id` and external references.
///
/// # Arguments
///
/// * `previous_data` - Optional YAML text expected to represent an `EHR_STATUS` mapping.
///   If provided, the existing EHR_STATUS is parsed and modified. If None, a new EHR_STATUS
///   is created from scratch.
/// * `ehr_id_str` - Optional EHR identifier as a string.
/// * `external_refs` - Optional subject external references (None for no references).
///
/// # Returns
///
/// Returns YAML text representing a valid [`EhrStatus`] on success.
///
/// # Errors
///
/// Returns [`OpenEhrError`] if:
/// - the YAML does not represent an `EHR_STATUS` mapping,
/// - any field has an unexpected type,
/// - any unknown keys are present (due to `#[serde(deny_unknown_fields)]`),
/// - both `previous_data` and `ehr_id_str` are None (cannot create EHR_STATUS without data or ID).
pub(crate) fn ehr_status_render(
    previous_data: Option<&str>,
    ehr_id: Option<&EhrId>,
    external_refs: Option<Vec<ExternalReference>>,
) -> Result<String, OpenEhrError> {
    let previous_yaml = previous_data.map(ehr_status_parse_full).transpose()?;

    if previous_yaml.is_none() && ehr_id.is_none() {
        return Err(OpenEhrError::Translation(
            "Cannot create EHR_STATUS: both previous_data and ehr_id are None".to_string(),
        ));
    }

    match previous_yaml {
        Some(mut yaml) => {
            // Only update ehr_id if provided
            if let Some(new_ehr_id) = ehr_id {
                yaml.ehr_id = HierObjectId {
                    value: new_ehr_id.as_str().to_string(),
                };
            }

            // Add to existing external refs instead of replacing
            let mut existing_refs = yaml.subject.external_ref.0.clone();
            let new_refs = external_refs
                .unwrap_or_default()
                .into_iter()
                .map(|ext_ref| PartyRef {
                    id: ObjectId {
                        value: ext_ref.id.simple().to_string(),
                    },
                    namespace: ext_ref.namespace,
                    type_: DEFAULT_EXTERNAL_REF_TYPE.to_string(),
                })
                .collect::<Vec<_>>();
            existing_refs.extend(new_refs);

            yaml.subject.external_ref = ExternalRefs(existing_refs);

            yaml.to_string()
        }
        None => ehr_status_init(ehr_id.unwrap(), external_refs).to_string(),
    }
}

/// Strictly parse an RM 1.1.0 `EHR_STATUS` from YAML text.
///
/// This uses `serde_path_to_error` to surface a best-effort "path" (e.g. `subject.external_ref`)
/// to the failing field when the YAML does not match the `EhrStatus` wire schema.
///
/// # Arguments
///
/// * `yaml_text` - YAML text expected to represent an `EHR_STATUS` mapping.
///
/// # Returns
///
/// Returns a valid [`EhrStatus`] on success.
///
/// # Errors
///
/// Returns [`OpenEhrError`] if:
/// - the YAML does not represent an `EHR_STATUS` mapping,
/// - any field has an unexpected type,
/// - any unknown keys are present (due to `#[serde(deny_unknown_fields)]`).
fn ehr_status_parse_full(yaml_text: &str) -> Result<EhrStatus, OpenEhrError> {
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
                "EHR_STATUS schema mismatch at {path}: {source}"
            )))
        }
    }
}

/// Create a new RM 1.1.0 `EHR_STATUS` wire struct from domain primitives.
///
/// This creates a new EhrStatus with default values for all fields except ehr_id and external_refs.
///
/// # Arguments
///
/// * `ehr_id` - EHR identifier.
/// * `external_refs` - Optional subject external references.
///
/// # Returns
///
/// Returns a new [`EhrStatus`] wire struct.
///
/// # Errors
///
/// This function does not return errors.
fn ehr_status_init(ehr_id: &EhrId, external_refs: Option<Vec<ExternalReference>>) -> EhrStatus {
    let external_refs = external_refs.unwrap_or_default();

    EhrStatus {
        rm_version: CURRENT_RM_VERSION,
        ehr_id: HierObjectId {
            value: ehr_id.as_str().to_string(),
        },
        archetype_node_id: DEFAULT_ARCHETYPE_NODE_ID.to_string(),
        name: DvText {
            value: DEFAULT_NAME.to_string(),
        },
        subject: PartySelf {
            external_ref: ExternalRefs(
                external_refs
                    .into_iter()
                    .map(|subject_ref| PartyRef {
                        id: ObjectId {
                            value: subject_ref.id.simple().to_string(),
                        },
                        namespace: subject_ref.namespace,
                        type_: DEFAULT_EXTERNAL_REF_TYPE.to_string(),
                    })
                    .collect(),
            ),
        },
        is_queryable: true,
        is_modifiable: true,
        other_details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_sample_yaml() {
        let input = r#"rm_version: rm_1_1_0
ehr_id:
    value: 1166765a406a4552ac9b8e141931a3dc

archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1

name:
    value: EHR Status

subject:
    external_ref:
        id:
            value: 2db695ed7cc04fc99b08e0c738069b71
        namespace: ehr://example.com/mpi
        type: PERSON

is_queryable: true
is_modifiable: true
"#;

        let component = ehr_status_parse_full(input).expect("parse yaml");
        let output = serde_yaml::to_string(&component).expect("write yaml");
        let reparsed = ehr_status_parse_full(&output).expect("reparse yaml");
        assert_eq!(component, reparsed);
    }

    #[test]
    fn strict_value_rejects_unknown_keys() {
        let input = r#"rm_version: rm_1_1_0
ehr_id:
    value: 1166765a406a4552ac9b8e141931a3dc

archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1

name:
    value: EHR Status

subject:
    external_ref:
        id:
            value: 2db695ed7cc04fc99b08e0c738069b71
        namespace: ehr://example.com/mpi
        type: PERSON

is_queryable: true
is_modifiable: true

unexpected_top_level_key: true
"#;

        let err = ehr_status_parse_full(input).expect_err("should reject unknown key");
        match err {
            OpenEhrError::Translation(msg) => {
                assert!(msg.contains("unexpected_top_level_key"));
                assert!(msg.contains("unknown field") || msg.contains("unknown variant"));
            }
            other => panic!("expected Translation error, got {other:?}"),
        }
    }

    #[test]
    fn strict_value_rejects_wrong_types() {
        let wrong_type = r#"rm_version: rm_1_1_0
ehr_id:
    value: 1166765a406a4552ac9b8e141931a3dc

archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1

name:
    value: EHR Status

is_queryable: "true"
is_modifiable: true
"#;

        let err = ehr_status_parse_full(wrong_type).expect_err("should reject wrong type");
        match err {
            OpenEhrError::Translation(msg) => {
                assert!(msg.contains("is_queryable"));
                assert!(msg.contains("invalid type") || msg.contains("expected"));
            }
            other => panic!("expected Translation error, got {other:?}"),
        }
    }

    #[test]
    fn parses_minimal_valid_yaml() {
        let minimal = r#"rm_version: rm_1_1_0
ehr_id:
    value: 1166765a406a4552ac9b8e141931a3dc

archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1

name:
    value: EHR Status

is_queryable: true
is_modifiable: true
"#;

        let result = ehr_status_parse_full(minimal).expect("should parse minimal YAML");
        assert_eq!(result.ehr_id.value, "1166765a406a4552ac9b8e141931a3dc");
        assert!(result.subject.external_ref.0.is_empty());
    }

    #[test]
    fn parses_with_multiple_external_refs() {
        let multiple_refs = r#"rm_version: rm_1_1_0
ehr_id:
    value: 1166765a406a4552ac9b8e141931a3dc

archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1

name:
    value: EHR Status

subject:
    external_ref:
        - id:
            value: 2db695ed7cc04fc99b08e0c738069b71
          namespace: ehr://example.com/mpi
          type: PERSON
        - id:
            value: 3fc695ed7cc04fc99b08e0c738069b72
          namespace: another-namespace
          type: PERSON

is_queryable: true
is_modifiable: true
"#;

        let result = ehr_status_parse_full(multiple_refs).expect("should parse multiple refs");
        assert_eq!(result.subject.external_ref.0.len(), 2);
    }

    #[test]
    fn rejects_invalid_uuid_in_domain_conversion() {
        let invalid_uuid = r#"rm_version: rm_1_1_0
ehr_id:
    value: not-a-uuid

archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1

name:
    value: EHR Status

is_queryable: true
is_modifiable: true
"#;

        let wire = ehr_status_parse_full(invalid_uuid).expect("should parse YAML structure");
        let err = uuid::Uuid::parse_str(&wire.ehr_id.value)
            .map_err(|_| OpenEhrError::Translation("ehr_id must be a valid UUID".to_string()))
            .expect_err("should reject invalid UUID in domain conversion");
        match err {
            OpenEhrError::Translation(msg) => {
                assert!(msg.contains("ehr_id must be a valid UUID"));
            }
            other => panic!("expected Translation error, got {other:?}"),
        }
    }

    #[test]
    fn ehr_status_render_modifies_fields() {
        let yaml = r#"rm_version: rm_1_1_0
ehr_id:
    value: 1166765a406a4552ac9b8e141931a3dc

archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1

name:
    value: EHR Status

subject:
    external_ref:
        id:
            value: 2db695ed7cc04fc99b08e0c738069b71
        namespace: ehr://existing-namespace
        type: PERSON

is_queryable: true
is_modifiable: true
"#;

        let new_ehr_id = EhrId("2166765a406a4552ac9b8e141931a3dc".to_string());
        let new_external_ref = ExternalReference {
            namespace: "ehr://new-namespace".to_string(),
            id: uuid::Uuid::parse_str("3db695ed7cc04fc99b08e0c738069b71").unwrap(),
        };

        let result_yaml = ehr_status_render(
            Some(yaml),
            Some(&new_ehr_id),
            Some(vec![new_external_ref.clone()]),
        )
        .expect("ehr_status_render should work");

        let result = ehr_status_parse_full(&result_yaml).expect("should parse returned YAML");

        // Check that the ehr_id was modified
        assert_eq!(result.ehr_id.value, "2166765a406a4552ac9b8e141931a3dc");

        // Check that external_ref was added to (not replaced)
        assert_eq!(result.subject.external_ref.0.len(), 2); // Should have 2 refs now
        assert_eq!(
            result.subject.external_ref.0[0].namespace,
            "ehr://existing-namespace"
        ); // Original preserved
        assert_eq!(
            result.subject.external_ref.0[1].namespace,
            "ehr://new-namespace"
        ); // New one added
    }

    #[test]
    fn ehr_status_render_handles_empty_external_refs() {
        let yaml = r#"rm_version: rm_1_1_0
ehr_id:
    value: 1166765a406a4552ac9b8e141931a3dc

archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1

name:
    value: EHR Status

subject:
    external_ref:
        id:
            value: 2db695ed7cc04fc99b08e0c738069b71
        namespace: ehr://old-namespace
        type: PERSON

is_queryable: true
is_modifiable: true
"#;

        let new_ehr_id = EhrId("3166765a406a4552ac9b8e141931a3dc".to_string());

        let result_yaml = ehr_status_render(Some(yaml), Some(&new_ehr_id), None)
            .expect("ehr_status_render should work with None external_refs");

        let result = ehr_status_parse_full(&result_yaml).expect("should parse returned YAML");

        // Check that the ehr_id was modified
        assert_eq!(result.ehr_id.value, "3166765a406a4552ac9b8e141931a3dc");

        // Check that external_ref was left unchanged (None means don't add anything)
        assert_eq!(result.subject.external_ref.0.len(), 1);
        assert_eq!(
            result.subject.external_ref.0[0].namespace,
            "ehr://old-namespace"
        );
    }

    #[test]
    fn ehr_status_init_builds_new_struct() {
        let ehr_id = EhrId("1166765a406a4552ac9b8e141931a3dc".to_string());
        let external_ref = ExternalReference {
            namespace: "ehr://test-namespace".to_string(),
            id: uuid::Uuid::parse_str("2db695ed7cc04fc99b08e0c738069b71").unwrap(),
        };

        let result = ehr_status_init(&ehr_id, Some(vec![external_ref.clone()]));

        // Check that the ehr_id was set
        assert_eq!(result.ehr_id.value, "1166765a406a4552ac9b8e141931a3dc");

        // Check default values
        assert_eq!(result.archetype_node_id, DEFAULT_ARCHETYPE_NODE_ID);
        assert_eq!(result.name.value, DEFAULT_NAME);
        assert!(result.is_queryable);
        assert!(result.is_modifiable);
        assert!(result.other_details.is_none());

        // Check that external_ref was set
        assert_eq!(result.subject.external_ref.0.len(), 1);
        assert_eq!(
            result.subject.external_ref.0[0].namespace,
            "ehr://test-namespace"
        );
        assert_eq!(
            result.subject.external_ref.0[0].id.value,
            "2db695ed7cc04fc99b08e0c738069b71"
        );
        assert_eq!(result.subject.external_ref.0[0].type_, "PERSON");
    }

    #[test]
    fn ehr_status_to_string_works() {
        let ehr_id = EhrId("1166765a406a4552ac9b8e141931a3dc".to_string());
        let external_ref = ExternalReference {
            namespace: "ehr://test-namespace".to_string(),
            id: uuid::Uuid::parse_str("2db695ed7cc04fc99b08e0c738069b71").unwrap(),
        };

        let ehr_status = ehr_status_init(&ehr_id, Some(vec![external_ref]));

        let yaml_string = ehr_status.to_string().expect("to_string should work");

        // Verify the YAML string contains expected content
        assert!(yaml_string.contains("ehr_id:"));
        assert!(yaml_string.contains("value: 1166765a406a4552ac9b8e141931a3dc"));
        assert!(yaml_string.contains("archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1"));
        assert!(yaml_string.contains("name:"));
        assert!(yaml_string.contains("value: EHR Status"));
        assert!(yaml_string.contains("subject:"));
        assert!(yaml_string.contains("external_ref:"));
        assert!(yaml_string.contains("namespace: ehr://test-namespace"));
        assert!(yaml_string.contains("is_queryable: true"));
        assert!(yaml_string.contains("is_modifiable: true"));

        // Verify it can be parsed back
        let reparsed =
            ehr_status_parse_full(&yaml_string).expect("should parse the generated YAML");
        assert_eq!(reparsed, ehr_status);
    }

    #[test]
    fn ehr_status_render_rejects_both_none() {
        let err = ehr_status_render(None, None, None).expect_err(
            "ehr_status_render should reject when both previous_data and ehr_id are None",
        );

        match err {
            OpenEhrError::Translation(msg) => {
                assert!(msg.contains("both previous_data and ehr_id are None"));
            }
            other => panic!("expected Translation error, got {other:?}"),
        }
    }

    #[test]
    fn ehr_status_render_rejects_none_previous_data_without_ehr_id() {
        let external_ref = ExternalReference {
            namespace: "ehr://test-namespace".to_string(),
            id: uuid::Uuid::parse_str("4db695ed7cc04fc99b08e0c738069b71").unwrap(),
        };

        let err = ehr_status_render(None, None, Some(vec![external_ref])).expect_err(
            "ehr_status_render should reject when previous_data is None but ehr_id is None",
        );
    }

    #[test]
    fn ehr_status_render_creates_new_from_scratch() {
        let ehr_id = EhrId("1166765a406a4552ac9b8e141931a3dc".to_string());
        let external_ref = ExternalReference {
            namespace: "ehr://test-namespace".to_string(),
            id: uuid::Uuid::parse_str("2db695ed7cc04fc99b08e0c738069b71").unwrap(),
        };

        let result_yaml = ehr_status_render(None, Some(&ehr_id), Some(vec![external_ref.clone()]))
            .expect("ehr_status_render should create new EHR_STATUS");

        let result = ehr_status_parse_full(&result_yaml).expect("should parse the result");

        assert_eq!(result.ehr_id.value, "1166765a406a4552ac9b8e141931a3dc");
        assert_eq!(result.archetype_node_id, DEFAULT_ARCHETYPE_NODE_ID);
        assert_eq!(result.name.value, DEFAULT_NAME);
        assert!(result.is_queryable);
        assert!(result.is_modifiable);
        assert!(result.other_details.is_none());
        assert_eq!(result.subject.external_ref.0.len(), 1);
        assert_eq!(
            result.subject.external_ref.0[0].namespace,
            "ehr://test-namespace"
        );
        assert_eq!(
            result.subject.external_ref.0[0].id.value,
            "2db695ed7cc04fc99b08e0c738069b71"
        );
        assert_eq!(result.subject.external_ref.0[0].type_, "PERSON");
    }
}
