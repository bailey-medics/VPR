# FHIR Integration

## Overview

The coordination repository uses **FHIR-aligned wire formats** for interoperability without implementing FHIR JSON, REST endpoints, or transport semantics.

This approach:

- Preserves FHIR semantic meaning
- Uses repository-based storage model
- Enables future FHIR projections
- Maintains human-readable formats
- Provides strict schema validation

---

## Coordination Status

### Overview

The `fhir::CoordinationStatus` module handles parsing and rendering of `COORDINATION_STATUS.yaml` files.

### API

```rust
// Parse from YAML
let status_data = fhir::CoordinationStatus::parse(yaml_text)?;

// Render to YAML
let yaml_text = fhir::CoordinationStatus::render(&status_data)?;
```

### Domain Types

- **`CoordinationStatusData`** - Top-level status structure
  - `coordination_id: Uuid` - Coordination repository identifier
  - `clinical_id: Uuid` - Linked clinical record identifier
  - `status: StatusInfo` - Status information

- **`StatusInfo`** - Status details
  - `lifecycle_state: LifecycleState` - Current lifecycle state
  - `record_open: bool` - Whether accepting new entries
  - `record_queryable: bool` - Whether queries are permitted
  - `record_modifiable: bool` - Whether modifications are permitted

- **`LifecycleState`** - Enumeration
  - `Active` - Operational and accepting updates
  - `Suspended` - Temporarily inactive
  - `Closed` - Permanently closed

### Validation

- UUID validation for `coordination_id` and `clinical_id`
- Enum validation for `lifecycle_state`
- Boolean validation for permission flags
- Strict schema with `deny_unknown_fields`

### Wire Format

Internal wire types use string UUIDs, translated to proper `Uuid` types at boundaries:

```rust
// Wire format (internal)
struct CoordinationStatusWire {
    coordination_id: String,
    clinical_id: String,
    status: StatusWire,
}

// Domain format (public)
struct CoordinationStatusData {
    coordination_id: Uuid,
    clinical_id: Uuid,
    status: StatusInfo,
}
```

---

## Thread Ledgers

### Overview

The `fhir::Messaging` module handles parsing and rendering of messaging thread `ledger.yaml` files.

This implementation uses FHIR Communication resource semantics without FHIR JSON transport.

### API

```rust
// Parse from YAML
let ledger_data = fhir::Messaging::ledger_parse(yaml_text)?;

// Render to YAML
let yaml_text = fhir::Messaging::ledger_render(&ledger_data)?;
```

### Domain Types

- **`LedgerData`** - Top-level ledger structure
  - `thread_id: TimestampId` - Thread identifier
  - `status: ThreadStatus` - Thread status
  - `created_at: DateTime<Utc>` - Creation timestamp
  - `last_updated_at: DateTime<Utc>` - Last update timestamp
  - `participants: Vec<LedgerParticipant>` - Participant list
  - `visibility: LedgerVisibility` - Visibility settings
  - `policies: LedgerPolicies` - Participation policies
  - `audit: LedgerAudit` - Change audit trail

- **`ThreadStatus`** - Enumeration
  - `Open` - Active, accepting messages
  - `Closed` - Closed to new messages
  - `Archived` - Archived (hidden from default views)

- **`LedgerParticipant`** - Participant information
  - `participant_id: Uuid` - Participant identifier
  - `role: ParticipantRole` - Participant role
  - `display_name: String` - Human-readable name
  - `organisation: Option<String>` - Organization affiliation

- **`ParticipantRole`** - Enumeration
  - `Clinician` - Clinical staff member
  - `Patient` - Patient participant
  - `CareTeam` - Care team member or healthcare professional
  - `System` - System-generated participant

- **`LedgerVisibility`** - Visibility settings
  - `sensitivity: String` - Sensitivity level (standard, confidential, restricted)
  - `restricted: bool` - Whether access is restricted beyond normal rules

- **`LedgerPolicies`** - Participation policies
  - `allow_patient_participation: bool` - Patient participation permitted
  - `allow_external_organisations: bool` - External organizations permitted

- **`LedgerAudit`** - Audit trail
  - `created_by: String` - Creator identifier
  - `change_log: Vec<AuditChangeLog>` - Chronological change log

- **`AuditChangeLog`** - Single audit entry
  - `changed_at: DateTime<Utc>` - Change timestamp
  - `changed_by: String` - Actor identifier
  - `description: String` - Human-readable description

### Validation

- UUID validation for `thread_id` (as `TimestampId`)
- UUID validation for all `participant_id` fields
- DateTime parsing with timezone handling
- Enum validation for `status` and `role` fields
- Strict schema with `deny_unknown_fields`

### Wire Format

Internal wire types separate concerns:

```rust
// Wire format (internal)
struct Ledger {
    thread_id: String,
    status: ThreadStatus,
    created_at: DateTime<Utc>,
    // ... string UUIDs, raw timestamps
}

// Domain format (public)
struct LedgerData {
    thread_id: TimestampId,
    status: ThreadStatus,
    created_at: DateTime<Utc>,
    // ... proper UUID types, validated timestamps
}
```

Translation happens at parse/render boundaries using internal helper functions.

---

## Wire Format Principles

### Separation of Concerns

- **Wire types** are internal implementation details
- **Domain types** are public API surface
- Translation happens at boundaries only
- Consumers work with domain types exclusively

### Strict Validation

All wire formats use `#[serde(deny_unknown_fields)]`:

- Unknown fields are rejected
- Prevents silent schema drift
- Ensures forward compatibility is explicit
- Catches typos and configuration errors

### Type Safety

- String identifiers validated and converted to proper types
- UUIDs parsed and validated at boundaries
- Timestamps validated and converted to `DateTime<Utc>`
- Enumerations validated against allowed values

### Human-Readable Formats

YAML is used for all wire formats:

- Git-friendly diffs
- Human-readable without tooling
- Suitable for manual review
- Easy to debug and inspect

### Error Handling

Parse errors use `serde_path_to_error` for detailed diagnostics:

```text
Thread ledger schema mismatch at participants[0].role:
unknown variant `doctor`, expected one of
`clinician`, `patient`, `careteam`, `system`
```

This provides:

- Precise error location in document
- Clear error description
- Expected values for enumerations
- Actionable feedback for corrections

---

## FHIR Alignment

### Conceptual Model

VPR coordination uses FHIR resource semantics:

- **COORDINATION_STATUS.yaml** ≈ FHIR operational status tracking
- **Thread ledger.yaml** ≈ FHIR Communication metadata
- **messages.md** ≈ FHIR Communication content

This is **conceptual alignment**, not implementation:

- No FHIR JSON format
- No FHIR REST endpoints
- No FHIR server behavior
- No FHIR Bundle/Transaction semantics

### Future Projections

FHIR-aligned wire formats enable future projections to:

- **FHIR Communication resources** - For messaging threads
- **FHIR Task resources** - For coordination tasks
- **FHIR DocumentReference** - For compositions
- **FHIR RESTful APIs** - For external integrations

Projection can happen:

- At API boundaries (gRPC/REST to FHIR)
- Via export tools (VPR to FHIR Bundle)
- Through ETL pipelines (VPR to FHIR data warehouse)

### Semantic Preservation

Key FHIR concepts preserved:

- **Communication.status** → `ThreadStatus` (open, closed, archived)
- **Communication.recipient** → `participants` with roles
- **Communication.sender** → author metadata in messages
- **Communication.sent** → `created_at` timestamp
- **Communication.payload** → message content in messages.md

This ensures:

- No semantic loss in translation
- Clear mapping to FHIR when needed
- Compatibility with FHIR-based systems
- Standards-based interoperability

---

## Implementation Details

### Module Structure

```
crates/fhir/src/
    lib.rs                    # Public exports and error types
    coordination_status.rs    # COORDINATION_STATUS.yaml handling
    messaging.rs              # Thread ledger.yaml handling
```

### Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum FhirError {
    InvalidInput(String),
    InvalidYaml(serde_yaml::Error),
    Translation(String),
    InvalidUuid(String),
    // ...
}
```

Errors are converted to `PatientError` at boundaries via `From` trait.

### Testing

Each module includes comprehensive tests:

- Round-trip parsing (parse → render → parse)
- Schema validation (reject unknown fields)
- Type validation (reject wrong types)
- UUID validation (reject malformed UUIDs)
- Enum validation (reject unknown variants)
- Edge cases (minimal valid documents, optional fields)

### Dependencies

- `serde` and `serde_yaml` - Serialization
- `serde_path_to_error` - Detailed error paths
- `chrono` - Timestamp handling
- `uuid` - UUID types
- `vpr_uuid` - TimestampId type

---

## Usage Examples

### Coordination Status

```rust
use fhir::{CoordinationStatus, CoordinationStatusData, StatusInfo, LifecycleState};

// Create new status
let status_data = CoordinationStatusData {
    coordination_id: Uuid::new_v4(),
    clinical_id: existing_clinical_uuid,
    status: StatusInfo {
        lifecycle_state: LifecycleState::Active,
        record_open: true,
        record_queryable: true,
        record_modifiable: true,
    },
};

// Render to YAML
let yaml = CoordinationStatus::render(&status_data)?;

// Write to file
fs::write("COORDINATION_STATUS.yaml", yaml)?;

// Later, parse back
let yaml_text = fs::read_to_string("COORDINATION_STATUS.yaml")?;
let parsed = CoordinationStatus::parse(&yaml_text)?;
assert_eq!(status_data, parsed);
```

### Thread Ledger

```rust
use fhir::{Messaging, LedgerData, ThreadStatus, LedgerParticipant, ParticipantRole};

// Create ledger
let ledger_data = LedgerData {
    thread_id: thread_id,
    status: ThreadStatus::Open,
    created_at: Utc::now(),
    last_updated_at: Utc::now(),
    participants: vec![
        LedgerParticipant {
            participant_id: clinician_uuid,
            role: ParticipantRole::Clinician,
            display_name: "Dr Jane Smith".to_string(),
            organisation: Some("Example NHS Trust".to_string()),
        },
    ],
    // ... visibility, policies, audit
};

// Render to YAML
let yaml = Messaging::ledger_render(&ledger_data)?;

// Write to file
fs::write("ledger.yaml", yaml)?;

// Later, parse back
let yaml_text = fs::read_to_string("ledger.yaml")?;
let parsed = Messaging::ledger_parse(&yaml_text)?;
```

---

## References

- [Coordination Index](index.md)
- [Messaging Design](messaging.md)
- [FHIR Communication Resource](https://hl7.org/fhir/communication.html)
