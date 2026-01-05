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

use crate::OpenehrError;
use serde::{Deserialize, Serialize};
use serde::{Deserializer, Serializer};
use std::fs;
use std::path::Path;

const DEFAULT_ARCHETYPE_NODE_ID: &str = "openEHR-EHR-STATUS.ehr_status.v1";
const DEFAULT_NAME: &str = "EHR Status";
const DEFAULT_SUBJECT_TYPE: &str = "PERSON";

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
/// Returns [`OpenehrError`] if:
/// - the YAML is invalid or does not match the expected wire schema.
pub fn read_yaml(yaml: &str) -> Result<EhrStatus, OpenehrError> {
    Ok(serde_yaml::from_str(yaml)?)
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
/// Returns [`OpenehrError`] if serialisation fails.
pub fn write_yaml(component: &EhrStatus) -> Result<String, OpenehrError> {
    Ok(serde_yaml::to_string(component)?)
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
/// Returns [`OpenehrError`] if:
/// - reading the existing YAML fails,
/// - serialisation/deserialisation fails,
/// - writing the updated YAML fails.
pub fn ehr_status_write(
    path: &Path,
    ehr_id: uuid::Uuid,
    external_reference: Option<Vec<crate::ExternalReference>>,
) -> Result<(), OpenehrError> {
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
                        type_: DEFAULT_SUBJECT_TYPE.to_string(),
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
/// Returns [`OpenehrError`] if:
/// - `ehr_id` is not a valid UUID,
/// - any `subject.external_ref.*.id.value` is not a valid UUID.
pub fn ehr_status_to_domain_parts(
    wire: &EhrStatus,
) -> Result<(uuid::Uuid, Vec<crate::ExternalReference>), OpenehrError> {
    let ehr_id = uuid::Uuid::parse_str(&wire.ehr_id.value)
        .map_err(|_| OpenehrError::Translation("ehr_id must be a valid UUID".to_string()))?;

    let mut subject_external_refs = Vec::new();
    for external_ref in &wire.subject.external_ref.0 {
        let id = uuid::Uuid::parse_str(&external_ref.id.value).map_err(|_| {
            OpenehrError::Translation(
                "subject.external_ref.id.value must be a valid UUID".to_string(),
            )
        })?;

        subject_external_refs.push(crate::ExternalReference {
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
}
