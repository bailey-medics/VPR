//! RM 1.1.0 `EHR_STATUS` wire model and translation helpers.
//!
//! This module defines the on-disk YAML representation VPR uses for an openEHR `EHR_STATUS`
//! component, aligned to the openEHR RM 1.x structure.
//!
//! Responsibilities:
//! - Define a strict wire model (`EhrStatus`) for serialisation/deserialisation.
//! - Preserve legacy YAML shapes where `subject.external_ref` may be absent, a single object,
//!   or a list.
//! - Provide translation helpers between VPR domain primitives and the wire model.
//!
//! Notes:
//! - Clinical meaning lives in `vpr-core`; this crate focuses on file formats and standards
//!   alignment.
//! - `ehr_status_write` performs a shallow YAML merge when the target file already exists:
//!   top-level mapping keys written by VPR are inserted/overwritten, while unrelated keys are
//!   preserved.

use crate::ExternalReference;
use crate::OpenEhrError;
use serde::{Deserialize, Serialize};
use serde::{Deserializer, Serializer};
use std::fs;
use std::path::Path;

const DEFAULT_ARCHETYPE_NODE_ID: &str = "openEHR-EHR-STATUS.ehr_status.v1";
const DEFAULT_NAME: &str = "EHR Status";
const DEFAULT_EXTERNAL_REF_TYPE: &str = "PERSON";

/// RM 1.x-aligned wire representation of `EHR_STATUS` for VPR on-disk YAML.
///
/// Notes:
/// - This is a wire model: it intentionally includes openEHR RM fields and types.
/// - Optional RM fields are represented as `Option<T>`.
/// - VPR persists an `ehr_id` wrapper at the top-level of this YAML, matching VPR's on-disk
///   component layout.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EhrStatus {
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
        write_yaml(self)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HierObjectId {
    pub value: String,
}

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
    fn is_empty(&self) -> bool {
        self.external_ref.is_empty()
    }
}

/// `subject.external_ref` may be absent, a single object, or a list.
///
/// VPR supports both forms to preserve compatibility with existing on-disk YAML.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExternalRefs(pub Vec<PartyRef>);

impl ExternalRefs {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(untagged)]
enum OneOrManyPartyRef {
    One(PartyRef),
    Many(Vec<PartyRef>),
}

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

/// RM `OBJECT_ID` (simplified to a `value` string wrapper).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ObjectId {
    pub value: String,
}

/// RM `ITEM_STRUCTURE` (highly constrained to the needs of VPR YAML).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ItemStructure {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Element>,
}

/// RM `ELEMENT` (constrained to `DV_TEXT` name/value for VPR YAML).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Element {
    pub name: DvText,
    pub value: DvText,
}

/// Read an RM 1.1.0 `EHR_STATUS` wire component from YAML.
///
/// # Arguments
///
/// * `yaml` - YAML document containing an `EHR_STATUS` component.
///
/// # Returns
///
/// Returns a parsed [`EhrStatus`] wire struct.
///
/// # Errors
///
/// Returns [`OpenEhrError`] if:
/// - the YAML is invalid or does not match the expected wire schema.
pub fn read_yaml(yaml: &str) -> Result<EhrStatus, OpenEhrError> {
    ehr_status_parse_full(yaml)
}

/// Write an RM 1.1.0 `EHR_STATUS` wire component to YAML.
///
/// # Arguments
///
/// * `component` - The wire struct to serialise.
///
/// # Returns
///
/// Returns a YAML string.
///
/// # Errors
///
/// Returns [`OpenEhrError`] if serialisation fails.
pub fn write_yaml(component: &EhrStatus) -> Result<String, OpenEhrError> {
    Ok(serde_yaml::to_string(component)?)
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
    ehr_id_str: Option<&str>,
    external_refs: Option<Vec<ExternalReference>>,
) -> Result<String, OpenEhrError> {
    let previous_yaml = previous_data.map(ehr_status_parse_full).transpose()?;

    let ehr_id = ehr_id_str.map(|id| HierObjectId {
        value: id.to_string(),
    });

    if previous_yaml.is_none() && ehr_id.is_none() {
        return Err(OpenEhrError::Translation(
            "Cannot create EHR_STATUS: both previous_data and ehr_id_str are None".to_string(),
        ));
    }

    match previous_yaml {
        Some(mut yaml) => {
            // Only update ehr_id if provided
            if let Some(ref new_ehr_id) = ehr_id {
                yaml.ehr_id = new_ehr_id.clone();
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
pub fn ehr_status_init(
    ehr_id: HierObjectId,
    external_refs: Option<Vec<ExternalReference>>,
) -> EhrStatus {
    let external_refs = external_refs.unwrap_or_default();

    EhrStatus {
        ehr_id,
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

fn merge_yaml_values(current: serde_yaml::Value, new_data: serde_yaml::Value) -> serde_yaml::Value {
    match (current, new_data) {
        (serde_yaml::Value::Null, new) => new,
        (serde_yaml::Value::Mapping(mut current_map), serde_yaml::Value::Mapping(new_map)) => {
            for (key, value) in new_map {
                current_map.insert(key, value);
            }
            serde_yaml::Value::Mapping(current_map)
        }
        (_, new) => new,
    }
}

/// Write an RM 1.x `EHR_STATUS` YAML file from VPR domain primitives.
///
/// Applies RM-required defaults in the wire layer and writes to disk.
/// If the file already exists, a shallow YAML merge is performed to preserve unrelated keys.
///
/// # Arguments
///
/// * `path` - Path to the target `ehr_status.yaml` file.
/// * `ehr_id` - EHR identifier for the record (written in canonical UUID form).
/// * `external_reference` - Optional subject external references.
///
/// # Returns
///
/// Returns `Ok(())` on success.
///
/// # Errors
///
/// Returns [`OpenEhrError`] if:
/// - reading the existing YAML fails,
/// - serialisation/deserialisation fails,
/// - writing the updated YAML fails.
pub(crate) fn ehr_status_write(
    path: &Path,
    ehr_id: uuid::Uuid,
    external_reference: Option<Vec<ExternalReference>>,
) -> Result<(), OpenEhrError> {
    let external_reference = external_reference.unwrap_or_default();

    let wire = EhrStatus {
        ehr_id: HierObjectId {
            value: ehr_id.simple().to_string(),
        },
        archetype_node_id: DEFAULT_ARCHETYPE_NODE_ID.to_string(),
        name: DvText {
            value: DEFAULT_NAME.to_string(),
        },
        subject: PartySelf {
            external_ref: ExternalRefs(
                external_reference
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
    };

    let new_value = serde_yaml::to_value(&wire)?;
    let current = if path.exists() {
        let yaml_str = fs::read_to_string(path)?;
        serde_yaml::from_str(&yaml_str)?
    } else {
        serde_yaml::Value::Null
    };

    let merged_value = merge_yaml_values(current, new_value);
    let yaml = serde_yaml::to_string(&merged_value)?;
    fs::write(path, yaml)?;
    Ok(())
}

/// Translate an RM 1.x `EHR_STATUS` wire struct into VPR domain primitives.
///
/// # Arguments
///
/// * `wire` - Parsed RM wire struct.
///
/// # Returns
///
/// Returns the `(ehr_id, subject_external_refs)` domain primitives.
///
/// # Errors
///
/// Returns [`OpenEhrError`] if:
/// - `ehr_id` is not a valid UUID,
/// - any `subject.external_ref.*.id.value` is not a valid UUID.
pub fn ehr_status_to_domain_parts(
    wire: &EhrStatus,
) -> Result<(uuid::Uuid, Vec<ExternalReference>), OpenEhrError> {
    let ehr_id = uuid::Uuid::parse_str(&wire.ehr_id.value)
        .map_err(|_| OpenEhrError::Translation("ehr_id must be a valid UUID".to_string()))?;

    let mut subject_external_refs = Vec::new();
    for external_ref in &wire.subject.external_ref.0 {
        let id = uuid::Uuid::parse_str(&external_ref.id.value).map_err(|_| {
            OpenEhrError::Translation(
                "subject.external_ref.id.value must be a valid UUID".to_string(),
            )
        })?;

        subject_external_refs.push(ExternalReference {
            namespace: external_ref.namespace.clone(),
            id,
        });
    }

    Ok((ehr_id, subject_external_refs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_sample_yaml() {
        let input = r#"ehr_id:
    value: 1166765a406a4552ac9b8e141931a3dc

archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1

name:
    value: EHR Status

subject:
    external_ref:
        id:
            value: 2db695ed7cc04fc99b08e0c738069b71
        namespace: vpr://vpr.dev.1/mpi
        type: PERSON

is_queryable: true
is_modifiable: true
"#;

        let component = read_yaml(input).expect("parse yaml");
        let output = write_yaml(&component).expect("write yaml");
        let reparsed = read_yaml(&output).expect("reparse yaml");
        assert_eq!(component, reparsed);
    }

    #[test]
    fn strict_value_rejects_unknown_keys() {
        let input = r#"ehr_id:
    value: 1166765a406a4552ac9b8e141931a3dc

archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1

name:
    value: EHR Status

subject:
    external_ref:
        id:
            value: 2db695ed7cc04fc99b08e0c738069b71
        namespace: vpr://vpr.dev.1/mpi
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
        let wrong_type = r#"ehr_id:
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
        let minimal = r#"ehr_id:
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
        let multiple_refs = r#"ehr_id:
    value: 1166765a406a4552ac9b8e141931a3dc

archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1

name:
    value: EHR Status

subject:
    external_ref:
        - id:
            value: 2db695ed7cc04fc99b08e0c738069b71
          namespace: vpr://vpr.dev.1/mpi
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
        let invalid_uuid = r#"ehr_id:
    value: not-a-uuid

archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1

name:
    value: EHR Status

is_queryable: true
is_modifiable: true
"#;

        let wire = ehr_status_parse_full(invalid_uuid).expect("should parse YAML structure");
        let err = ehr_status_to_domain_parts(&wire)
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
        let yaml = r#"ehr_id:
    value: 1166765a406a4552ac9b8e141931a3dc

archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1

name:
    value: EHR Status

subject:
    external_ref:
        id:
            value: 2db695ed7cc04fc99b08e0c738069b71
        namespace: vpr://existing-namespace
        type: PERSON

is_queryable: true
is_modifiable: true
"#;

        let new_ehr_id = HierObjectId {
            value: "2166765a406a4552ac9b8e141931a3dc".to_string(),
        };
        let new_external_ref = ExternalReference {
            namespace: "vpr://new-namespace".to_string(),
            id: uuid::Uuid::parse_str("3db695ed7cc04fc99b08e0c738069b71").unwrap(),
        };

        let result_yaml = ehr_status_render(
            Some(yaml),
            Some(new_ehr_id.value.as_str()),
            Some(vec![new_external_ref.clone()]),
        )
        .expect("ehr_status_render should work");

        let result = read_yaml(&result_yaml).expect("should parse returned YAML");

        // Check that the ehr_id was modified
        assert_eq!(result.ehr_id.value, "2166765a406a4552ac9b8e141931a3dc");

        // Check that external_ref was added to (not replaced)
        assert_eq!(result.subject.external_ref.0.len(), 2); // Should have 2 refs now
        assert_eq!(
            result.subject.external_ref.0[0].namespace,
            "vpr://existing-namespace"
        ); // Original preserved
        assert_eq!(
            result.subject.external_ref.0[1].namespace,
            "vpr://new-namespace"
        ); // New one added
    }

    #[test]
    fn ehr_status_render_handles_empty_external_refs() {
        let yaml = r#"ehr_id:
    value: 1166765a406a4552ac9b8e141931a3dc

archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1

name:
    value: EHR Status

subject:
    external_ref:
        id:
            value: 2db695ed7cc04fc99b08e0c738069b71
        namespace: vpr://old-namespace
        type: PERSON

is_queryable: true
is_modifiable: true
"#;

        let new_ehr_id = HierObjectId {
            value: "3166765a406a4552ac9b8e141931a3dc".to_string(),
        };

        let result_yaml = ehr_status_render(Some(yaml), Some(new_ehr_id.value.as_str()), None)
            .expect("ehr_status_render should work with None external_refs");

        let result = read_yaml(&result_yaml).expect("should parse returned YAML");

        // Check that the ehr_id was modified
        assert_eq!(result.ehr_id.value, "3166765a406a4552ac9b8e141931a3dc");

        // Check that external_ref was left unchanged (None means don't add anything)
        assert_eq!(result.subject.external_ref.0.len(), 1);
        assert_eq!(
            result.subject.external_ref.0[0].namespace,
            "vpr://old-namespace"
        );
    }

    #[test]
    fn ehr_status_init_builds_new_struct() {
        let ehr_id = HierObjectId {
            value: "1166765a406a4552ac9b8e141931a3dc".to_string(),
        };
        let external_ref = ExternalReference {
            namespace: "vpr://test-namespace".to_string(),
            id: uuid::Uuid::parse_str("2db695ed7cc04fc99b08e0c738069b71").unwrap(),
        };

        let result = ehr_status_init(ehr_id.clone(), Some(vec![external_ref.clone()]));

        // Check that the ehr_id was set
        assert_eq!(result.ehr_id.value, "1166765a406a4552ac9b8e141931a3dc");

        // Check default values
        assert_eq!(result.archetype_node_id, DEFAULT_ARCHETYPE_NODE_ID);
        assert_eq!(result.name.value, DEFAULT_NAME);
        assert_eq!(result.is_queryable, true);
        assert_eq!(result.is_modifiable, true);
        assert!(result.other_details.is_none());

        // Check that external_ref was set
        assert_eq!(result.subject.external_ref.0.len(), 1);
        assert_eq!(
            result.subject.external_ref.0[0].namespace,
            "vpr://test-namespace"
        );
        assert_eq!(
            result.subject.external_ref.0[0].id.value,
            "2db695ed7cc04fc99b08e0c738069b71"
        );
        assert_eq!(result.subject.external_ref.0[0].type_, "PERSON");
    }

    #[test]
    fn ehr_status_to_string_works() {
        let ehr_id = HierObjectId {
            value: "1166765a406a4552ac9b8e141931a3dc".to_string(),
        };
        let external_ref = ExternalReference {
            namespace: "vpr://test-namespace".to_string(),
            id: uuid::Uuid::parse_str("2db695ed7cc04fc99b08e0c738069b71").unwrap(),
        };

        let ehr_status = ehr_status_init(ehr_id, Some(vec![external_ref]));

        let yaml_string = ehr_status.to_string().expect("to_string should work");

        // Verify the YAML string contains expected content
        assert!(yaml_string.contains("ehr_id:"));
        assert!(yaml_string.contains("value: 1166765a406a4552ac9b8e141931a3dc"));
        assert!(yaml_string.contains("archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1"));
        assert!(yaml_string.contains("name:"));
        assert!(yaml_string.contains("value: EHR Status"));
        assert!(yaml_string.contains("subject:"));
        assert!(yaml_string.contains("external_ref:"));
        assert!(yaml_string.contains("namespace: vpr://test-namespace"));
        assert!(yaml_string.contains("is_queryable: true"));
        assert!(yaml_string.contains("is_modifiable: true"));

        // Verify it can be parsed back
        let reparsed = read_yaml(&yaml_string).expect("should parse the generated YAML");
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
            namespace: "vpr://test-namespace".to_string(),
            id: uuid::Uuid::parse_str("4db695ed7cc04fc99b08e0c738069b71").unwrap(),
        };

        let err = ehr_status_render(None, None, Some(vec![external_ref])).expect_err(
            "ehr_status_render should reject when previous_data is None but ehr_id is None",
        );

        match err {
            OpenEhrError::Translation(msg) => {
                assert!(msg.contains("both previous_data and ehr_id are None"));
            }
            other => panic!("expected Translation error, got {other:?}"),
        }
    }
}
