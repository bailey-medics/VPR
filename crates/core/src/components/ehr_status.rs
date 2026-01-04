//! EHR status domain model.
//!
//! VPR is openEHR-aligned, but does **not** model the openEHR Reference Model internally.
//! This module contains the small, explicit, human-readable domain structs that VPR reasons
//! about.
//!
//! openEHR wire/serialisation concerns (DV_* wrappers, RM flags, archetype metadata, etc) live
//! outside the domain model.

use uuid::Uuid;

/// VPR's internal representation of an EHR_STATUS.
///
/// Only includes fields VPR creates, owns, alters, or reasons about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EhrStatus {
    /// EHR identifier.
    pub ehr_id: Uuid,

    /// Subject references that VPR associates with the EHR.
    ///
    /// VPR stores this as a list to allow for future expansion, even though the current openEHR
    /// serialisation only supports a single `subject.external_ref`.
    pub subject: Vec<SubjectRef>,
}

/// Simplified subject reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubjectRef {
    pub namespace: String,
    pub id: Uuid,
}
