use crate::OpenehrError;
use serde::{Deserialize, Serialize};
use serde::{Deserializer, Serializer};

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
pub fn read_yaml(yaml: &str) -> Result<EhrStatus, OpenehrError> {
    Ok(serde_yaml::from_str(yaml)?)
}

/// Write an RM 1.1.0 `EHR_STATUS` wire component to YAML.
pub fn write_yaml(component: &EhrStatus) -> Result<String, OpenehrError> {
    Ok(serde_yaml::to_string(component)?)
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
