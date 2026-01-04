//! VPR-specific helpers for openEHR wire types.
//!
//! This module contains explicit translation helpers between VPR domain data (UUIDs and subject
//! references) and openEHR RM 1.x wire structs.
//!
//! Domain structs live in `vpr-core`. This crate does not depend on them (to avoid cycles), so
//! translations here operate on primitives.

use crate::rm_1_1_0::ehr_status as wire;
use crate::OpenehrError;

const DEFAULT_ARCHETYPE_NODE_ID: &str = "openEHR-EHR-STATUS.ehr_status.v1";
const DEFAULT_NAME: &str = "EHR Status";
const DEFAULT_SUBJECT_TYPE: &str = "PERSON";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubjectExternalRef {
    pub namespace: String,
    pub id: uuid::Uuid,
}

/// Translate VPR domain primitives into an RM 1.x `EHR_STATUS` wire struct.
///
/// Applies RM-required defaults in the wire layer.
pub fn ehr_status_from_domain_parts(
    ehr_id: uuid::Uuid,
    subject: Option<Vec<SubjectExternalRef>>,
) -> wire::EhrStatus {
    let subject = subject.unwrap_or_default();
    wire::EhrStatus {
        ehr_id: wire::HierObjectId {
            value: ehr_id.simple().to_string(),
        },
        archetype_node_id: DEFAULT_ARCHETYPE_NODE_ID.to_string(),
        name: wire::DvText {
            value: DEFAULT_NAME.to_string(),
        },
        subject: wire::PartySelf {
            external_ref: wire::ExternalRefs(
                subject
                    .into_iter()
                    .map(|subject_ref| wire::PartyRef {
                        id: wire::ObjectId {
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
    }
}

/// Translate an RM 1.x `EHR_STATUS` wire struct into VPR domain primitives.
///
/// Validates required domain fields (UUID parsing) while translating.
pub fn ehr_status_to_domain_parts(
    wire: &wire::EhrStatus,
) -> Result<(uuid::Uuid, Vec<SubjectExternalRef>), OpenehrError> {
    let ehr_id = uuid::Uuid::parse_str(&wire.ehr_id.value)
        .map_err(|_| OpenehrError::Translation("ehr_id must be a valid UUID".to_string()))?;

    let mut subject_external_refs = Vec::new();
    for external_ref in &wire.subject.external_ref.0 {
        let id = uuid::Uuid::parse_str(&external_ref.id.value).map_err(|_| {
            OpenehrError::Translation(
                "subject.external_ref.id.value must be a valid UUID".to_string(),
            )
        })?;

        subject_external_refs.push(SubjectExternalRef {
            namespace: external_ref.namespace.clone(),
            id,
        });
    }

    Ok((ehr_id, subject_external_refs))
}
