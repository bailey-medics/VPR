//! FHIR-aligned messaging wire models and translation helpers.
//!
//! This module provides both domain-level types and wire models for messaging thread ledgers,
//! which store contextual and policy metadata (not clinical narrative).
//!
//! Responsibilities:
//! - Define public domain-level types for external API use
//! - Define a strict wire model (`Ledger`) for serialisation/deserialisation
//! - Provide translation helpers between domain primitives and the wire model
//! - Validate ledger structure and enforce required fields
//!
//! Notes:
//! - Clinical messages are stored separately in thread.md
//! - This ledger is mutable and overwriteable (unlike messages)
//! - Changes are git-audited via the change_log

use crate::{FhirError, TimestampId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vpr_types::NonEmptyText;

// ============================================================================
// Public exports for external use
// ============================================================================
// These re-exports provide messaging-focused naming conventions for external consumers
// while maintaining FHIR-aligned internal naming (MessageParticipant, ParticipantRole).

pub use MessageParticipant as MessageAuthor;
pub use ParticipantRole as AuthorRole;

// ============================================================================
// Public domain-level types
// ============================================================================

/// Public API struct for thread ledger data with flattened fields.
///
/// This struct provides a flat structure suitable for API consumption,
/// without nested visibility or policies objects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LedgerData {
    /// Unique identifier for this communication (timestamp-prefixed UUID).
    pub communication_id: TimestampId,

    /// Current status of the thread.
    pub status: ThreadStatus,

    /// Timestamp when the thread was created.
    pub created_at: DateTime<Utc>,

    /// Timestamp when the thread was last updated.
    pub last_updated_at: DateTime<Utc>,

    /// Participants in this thread with their roles and display information.
    pub participants: Vec<MessageParticipant>,

    /// Sensitivity level for this thread.
    pub sensitivity: SensitivityLevel,

    /// Whether access is restricted beyond normal rules.
    pub restricted: bool,

    /// Whether patient participation is allowed.
    pub allow_patient_participation: bool,

    /// Whether external organisations can participate.
    pub allow_external_organisations: bool,
}

impl LedgerData {
    /// Converts this flat structure to the nested LedgerDataNested format.
    fn to_ledger_data_nested(&self) -> LedgerDataNested {
        LedgerDataNested {
            communication_id: self.communication_id.clone(),
            status: self.status.clone(),
            created_at: self.created_at,
            last_updated_at: self.last_updated_at,
            participants: self.participants.clone(),
            visibility: LedgerVisibility {
                sensitivity: self.sensitivity,
                restricted: self.restricted,
            },
            policies: LedgerPolicies {
                allow_patient_participation: self.allow_patient_participation,
                allow_external_organisations: self.allow_external_organisations,
            },
        }
    }

    /// Creates a flat structure from the nested LedgerDataNested format.
    fn from_ledger_data_nested(data: LedgerDataNested) -> Self {
        Self {
            communication_id: data.communication_id,
            status: data.status,
            created_at: data.created_at,
            last_updated_at: data.last_updated_at,
            participants: data.participants,
            sensitivity: data.visibility.sensitivity,
            restricted: data.visibility.restricted,
            allow_patient_participation: data.policies.allow_patient_participation,
            allow_external_organisations: data.policies.allow_external_organisations,
        }
    }
}

/// Internal nested carrier for thread ledger data.
///
/// This struct represents the contextual and policy metadata for a messaging thread
/// with nested structures that match the YAML wire format.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LedgerDataNested {
    /// Unique identifier for this communication (timestamp-prefixed UUID).
    pub communication_id: TimestampId,

    /// Current status of the thread.
    pub status: ThreadStatus,

    /// Timestamp when the thread was created.
    pub created_at: DateTime<Utc>,

    /// Timestamp when the thread was last updated.
    pub last_updated_at: DateTime<Utc>,

    /// Participants in this thread with their roles and display information.
    pub participants: Vec<MessageParticipant>,

    /// Visibility and sensitivity settings for this thread.
    pub visibility: LedgerVisibility,

    /// Thread policies controlling participation and access.
    pub policies: LedgerPolicies,
}

/// Thread status enumeration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThreadStatus {
    /// Thread is active and accepting new messages.
    Open,
    /// Thread is closed to new messages but remains readable.
    Closed,
    /// Thread is archived (typically not shown in default views).
    Archived,
}

/// A message participant in a messaging thread.
///
/// This represents a participant in a messaging thread with their identity,
/// display name, and role.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageParticipant {
    /// Unique identifier for this participant (UUID).
    pub id: Uuid,

    /// Human-readable display name for this participant.
    pub name: NonEmptyText,

    /// Role of this participant in the conversation.
    pub role: ParticipantRole,
}

/// Role of a participant in a messaging thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParticipantRole {
    /// Clinical staff member.
    Clinician,
    /// Care administrator or coordinator.
    CareAdministrator,
    /// Patient participant.
    Patient,
    /// Patient associate (family member, carer, or authorized representative).
    PatientAssociate,
    /// System-generated participant (for automated messages).
    System,
}

/// Sensitivity level for thread visibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SensitivityLevel {
    /// Standard sensitivity - normal access controls apply.
    Standard,
    /// Confidential - restricted to care team only.
    Confidential,
    /// Restricted - highly sensitive, minimal access.
    Restricted,
}

impl SensitivityLevel {
    /// Parses a sensitivity level from its string representation.
    pub fn parse(s: &str) -> Result<Self, FhirError> {
        match s.to_lowercase().as_str() {
            "standard" => Ok(Self::Standard),
            "confidential" => Ok(Self::Confidential),
            "restricted" => Ok(Self::Restricted),
            _ => Err(FhirError::InvalidInput(format!(
                "Invalid sensitivity level: {}",
                s
            ))),
        }
    }

    /// Returns the string representation of this sensitivity level.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Confidential => "confidential",
            Self::Restricted => "restricted",
        }
    }
}

impl ParticipantRole {
    /// Parses a role from its string representation.
    ///
    /// # Arguments
    ///
    /// * `s` - NonEmptyText representation of the role (case-insensitive)
    ///
    /// # Returns
    ///
    /// Returns the matching `ParticipantRole` variant.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError::InvalidInput`] if the string does not match any known role.
    pub fn parse(s: &str) -> Result<Self, FhirError> {
        match s.to_lowercase().as_str() {
            "clinician" => Ok(Self::Clinician),
            "careadministrator" => Ok(Self::CareAdministrator),
            "patient" => Ok(Self::Patient),
            "patientassociate" => Ok(Self::PatientAssociate),
            "system" => Ok(Self::System),
            _ => Err(FhirError::InvalidInput(format!("Invalid role: {}", s))),
        }
    }
}

/// Visibility and sensitivity settings for a thread.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerVisibility {
    /// Sensitivity level (standard, confidential, restricted).
    pub sensitivity: SensitivityLevel,

    /// Whether access is restricted beyond normal rules.
    pub restricted: bool,
}

/// Thread policies controlling participation and access.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerPolicies {
    /// Whether patient participation is allowed.
    pub allow_patient_participation: bool,

    /// Whether external organisations can participate.
    pub allow_external_organisations: bool,
}

// ============================================================================
// Public Messaging operations
// ============================================================================

/// Messaging operations for thread ledgers.
///
/// This is a zero-sized type used for namespacing messaging-related operations.
/// All methods are associated functions.
pub struct Messaging;

impl Messaging {
    /// Parse a thread ledger from YAML text.
    ///
    /// This uses `serde_path_to_error` to surface a best-effort "path" (e.g. `participants[0].role`)
    /// to the failing field when the YAML does not match the `Ledger` wire schema.
    ///
    /// # Arguments
    ///
    /// * `yaml_text` - YAML text expected to represent a thread ledger mapping.
    ///
    /// # Returns
    ///
    /// Returns a [`LedgerData`] with domain-level fields extracted from the ledger.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError`] if:
    /// - the YAML does not represent a valid thread ledger,
    /// - any field has an unexpected type,
    /// - any unknown keys are present (due to `#[serde(deny_unknown_fields)]`),
    /// - thread_id is not a valid TimestampId,
    /// - participant_id values are not valid UUIDs.
    pub fn ledger_parse(yaml_text: &str) -> Result<LedgerData, FhirError> {
        let deserializer = serde_yaml::Deserializer::from_str(yaml_text);

        let wire = match serde_path_to_error::deserialize::<_, Ledger>(deserializer) {
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
                    "Thread ledger schema mismatch at {path}: {source}"
                )));
            }
        };

        // Convert wire format to domain types
        let ledger_data_nested = wire_to_domain(wire)?;
        Ok(LedgerData::from_ledger_data_nested(ledger_data_nested))
    }

    /// Render a thread ledger as YAML text.
    ///
    /// This converts domain-level [`LedgerData`] into wire format and serializes to YAML.
    ///
    /// # Arguments
    ///
    /// * `ledger` - Thread ledger data containing all fields.
    ///
    /// # Returns
    ///
    /// Returns a YAML string representation of the ledger.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError`] if serialization fails.
    pub fn ledger_render(ledger: &LedgerData) -> Result<String, FhirError> {
        let ledger_data = ledger.to_ledger_data_nested();
        let wire: Ledger = domain_to_wire(&ledger_data);
        serde_yaml::to_string(&wire)
            .map_err(|e| FhirError::Translation(format!("Failed to serialize ledger: {e}")))
    }
}

// ============================================================================
// Wire types (internal)
// ============================================================================

/// Wire representation of a thread ledger for on-disk YAML.
///
/// This is the exact structure that will be serialized to/from YAML.
/// All fields use `#[serde(deny_unknown_fields)]` for strict validation.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Ledger {
    pub communication_id: String,
    pub status: ThreadStatus,
    pub created_at: DateTime<Utc>,
    pub last_updated_at: DateTime<Utc>,
    pub participants: Vec<Participant>,
    pub visibility: Visibility,
    pub policies: Policies,
}

/// Wire representation of a participant.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Participant {
    pub participant_id: String,
    pub display_name: String,
    pub role: ParticipantRole,
}

/// Wire representation of visibility settings.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Visibility {
    pub sensitivity: String,
    pub restricted: bool,
}

/// Wire representation of thread policies.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Policies {
    pub allow_patient_participation: bool,
    pub allow_external_organisations: bool,
}

// ============================================================================
// Helper functions (internal)
// ============================================================================

/// Convert wire format ledger to domain types.
///
/// This performs validation and conversion of string identifiers to proper types.
fn wire_to_domain(wire: Ledger) -> Result<LedgerDataNested, FhirError> {
    // Parse communication_id as TimestampId
    let communication_id = wire
        .communication_id
        .parse::<TimestampId>()
        .map_err(|e| FhirError::InvalidInput(format!("Invalid communication_id: {e}")))?;

    // Convert participants, validating UUIDs
    let mut participants = Vec::with_capacity(wire.participants.len());
    for (idx, p) in wire.participants.iter().enumerate() {
        let participant_id = Uuid::parse_str(&p.participant_id).map_err(|_| {
            FhirError::InvalidUuid(format!(
                "Invalid UUID in participants[{idx}].participant_id: {}",
                p.participant_id
            ))
        })?;

        participants.push(MessageParticipant {
            id: participant_id,
            name: NonEmptyText::new(&p.display_name).map_err(|_| {
                FhirError::Translation(format!("Empty display name in participants[{idx}]"))
            })?,
            role: p.role,
        });
    }

    Ok(LedgerDataNested {
        communication_id,
        status: wire.status,
        created_at: wire.created_at,
        last_updated_at: wire.last_updated_at,
        participants,
        visibility: LedgerVisibility {
            sensitivity: SensitivityLevel::parse(&wire.visibility.sensitivity)?,
            restricted: wire.visibility.restricted,
        },
        policies: LedgerPolicies {
            allow_patient_participation: wire.policies.allow_patient_participation,
            allow_external_organisations: wire.policies.allow_external_organisations,
        },
    })
}

/// Convert domain types to wire format ledger.
fn domain_to_wire(data: &LedgerDataNested) -> Ledger {
    Ledger {
        communication_id: data.communication_id.to_string(),
        status: data.status.clone(),
        created_at: data.created_at,
        last_updated_at: data.last_updated_at,
        participants: data
            .participants
            .iter()
            .map(|p| Participant {
                participant_id: p.id.to_string(),
                display_name: p.name.to_string(),
                role: p.role,
            })
            .collect(),
        visibility: Visibility {
            sensitivity: data.visibility.sensitivity.as_str().to_string(),
            restricted: data.visibility.restricted,
        },
        policies: Policies {
            allow_patient_participation: data.policies.allow_patient_participation,
            allow_external_organisations: data.policies.allow_external_organisations,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_sample_yaml() {
        let input = r#"communication_id: 20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000
status: open
created_at: "2026-01-11T14:35:22.045Z"
last_updated_at: "2026-01-11T15:10:04.912Z"
participants:
  - participant_id: 4f8c2a1d-9e3b-4a7c-8f1e-6b0d2c5a9f12
    role: clinician
    display_name: Dr Jane Smith
  - participant_id: a1d3c5e7-f9b2-4680-b2d4-f6e8c0a9d1e3
    role: clinician
    display_name: Dr Tom Patel
  - participant_id: 9b7c6d5e-4f3a-2b1c-0e8d-7f6a5b4c3d2e
    role: patient
    display_name: John Doe
visibility:
  sensitivity: standard
  restricted: false
policies:
  allow_patient_participation: true
  allow_external_organisations: false
"#;

        let ledger_data = Messaging::ledger_parse(input).expect("parse yaml");
        let output = Messaging::ledger_render(&ledger_data).expect("render ledger");
        let reparsed = Messaging::ledger_parse(&output).expect("reparse yaml");
        assert_eq!(ledger_data, reparsed);
    }

    #[test]
    fn strict_validation_rejects_unknown_keys() {
        let input = r#"communication_id: 20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000
status: open
created_at: "2026-01-11T14:35:22.045Z"
last_updated_at: "2026-01-11T15:10:04.912Z"
participants: []
visibility:
  sensitivity: standard
  restricted: false
policies:
  allow_patient_participation: true
  allow_external_organisations: false
unexpected_key: should_fail
"#;

        let err = Messaging::ledger_parse(input).expect_err("should reject unknown key");
        match err {
            FhirError::Translation(msg) => {
                assert!(msg.contains("unexpected_key"));
            }
            other => panic!("expected Translation error, got {other:?}"),
        }
    }

    #[test]
    fn strict_validation_rejects_wrong_types() {
        let input = r#"communication_id: 20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000
status: open
created_at: "2026-01-11T14:35:22.045Z"
last_updated_at: "2026-01-11T15:10:04.912Z"
participants: []
visibility:
  sensitivity: standard
  restricted: "not_a_boolean"
policies:
  allow_patient_participation: true
  allow_external_organisations: false
"#;

        let err = Messaging::ledger_parse(input).expect_err("should reject wrong type");
        match err {
            FhirError::Translation(msg) => {
                assert!(msg.contains("restricted"));
            }
            other => panic!("expected Translation error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_communication_id() {
        let input = r#"communication_id: not-a-valid-timestamp-id
status: open
created_at: "2026-01-11T14:35:22.045Z"
last_updated_at: "2026-01-11T15:10:04.912Z"
participants: []
visibility:
  sensitivity: standard
  restricted: false
policies:
  allow_patient_participation: true
  allow_external_organisations: false
"#;

        let err =
            Messaging::ledger_parse(input).expect_err("should reject invalid communication_id");
        match err {
            FhirError::InvalidInput(msg) => {
                assert!(msg.contains("Invalid communication_id"));
            }
            other => panic!("expected InvalidInput error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_participant_uuid() {
        let input = r#"communication_id: 20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000
status: open
created_at: "2026-01-11T14:35:22.045Z"
last_updated_at: "2026-01-11T15:10:04.912Z"
participants:
  - participant_id: not-a-valid-uuid
    role: clinician
    display_name: Dr Jane Smith
visibility:
  sensitivity: standard
  restricted: false
policies:
  allow_patient_participation: true
  allow_external_organisations: false
"#;

        let err =
            Messaging::ledger_parse(input).expect_err("should reject invalid participant UUID");
        match err {
            FhirError::InvalidUuid(msg) => {
                assert!(msg.contains("participants[0].participant_id"));
            }
            other => panic!("expected InvalidUuid error, got {other:?}"),
        }
    }

    #[test]
    fn parses_minimal_valid_ledger() {
        let input = r#"communication_id: 20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000
status: open
created_at: "2026-01-11T14:35:22.045Z"
last_updated_at: "2026-01-11T15:10:04.912Z"
participants: []
visibility:
  sensitivity: standard
  restricted: false
policies:
  allow_patient_participation: false
  allow_external_organisations: false
"#;

        let result = Messaging::ledger_parse(input).expect("should parse minimal ledger");
        assert_eq!(
            result.communication_id.to_string(),
            "20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(result.status, ThreadStatus::Open);
        assert!(result.participants.is_empty());
        assert_eq!(result.sensitivity, SensitivityLevel::Standard);
        assert!(!result.restricted);
    }

    #[test]
    fn handles_multiple_participants() {
        let input = r#"communication_id: 20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000
status: open
created_at: "2026-01-11T14:35:22.045Z"
last_updated_at: "2026-01-11T15:10:04.912Z"
participants:
  - participant_id: 4f8c2a1d-9e3b-4a7c-8f1e-6b0d2c5a9f12
    role: clinician
    display_name: Dr Jane Smith
  - participant_id: a1d3c5e7-f9b2-4680-b2d4-f6e8c0a9d1e3
    role: patient
    display_name: John Doe
visibility:
  sensitivity: standard
  restricted: false
policies:
  allow_patient_participation: true
  allow_external_organisations: false
"#;

        let result = Messaging::ledger_parse(input).expect("should parse multiple participants");
        assert_eq!(result.participants.len(), 2);
        assert_eq!(result.participants[0].role, ParticipantRole::Clinician);
        assert_eq!(result.participants[1].role, ParticipantRole::Patient);
    }

    #[test]
    fn handles_closed_and_archived_status() {
        let closed = r#"communication_id: 20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000
status: closed
created_at: "2026-01-11T14:35:22.045Z"
last_updated_at: "2026-01-11T15:10:04.912Z"
participants: []
visibility:
  sensitivity: standard
  restricted: false
policies:
  allow_patient_participation: true
  allow_external_organisations: false
"#;

        let result = Messaging::ledger_parse(closed).expect("should parse closed status");
        assert_eq!(result.status, ThreadStatus::Closed);

        let archived = closed.replace("status: closed", "status: archived");
        let result = Messaging::ledger_parse(&archived).expect("should parse archived status");
        assert_eq!(result.status, ThreadStatus::Archived);
    }
}
