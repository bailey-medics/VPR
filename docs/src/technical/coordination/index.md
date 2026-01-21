# Care Coordination Repository

## Overview

The Care Coordination Repository manages **coordination state** separate from clinical records.

It handles workflow coordination, cross-system state, and operational metadata that supports clinical care delivery without containing clinical content itself.

---

## Repository Structure

The coordination repository follows the same sharded structure as clinical records:

```text
patient_data/
  coordination/
    <s1>/
      <s2>/
        <uuid>/
          .git/
          COORDINATION_STATUS.yaml
          communications/
            <thread-id>/
              messages.md
              ledger.yaml
          encounters/
            ...
          appointments/
            ...
```

---

## Root Status File

### COORDINATION_STATUS.yaml

Each coordination repository includes a root status file that links it to the associated clinical record:

```yaml
coordination_id: "7f4c2e9d-4b0a-4f3a-9a2c-0e9a6b5d1c88"
clinical_id: "a4f91c6d-3b2e-4c5f-9d7a-1e8b6c0a9f12"
status:
  lifecycle_state: active # active | suspended | closed
  record_open: true
  record_queryable: true
  record_modifiable: true
```

**Purpose:**

- Links coordination record to clinical record via `clinical_id`
- Tracks lifecycle state of the coordination repository
- Controls operational permissions (queryable, modifiable)
- Created during coordination repository initialization

**Lifecycle states:**

- **active**: Coordination record is operational and accepting updates
- **suspended**: Temporarily inactive (e.g., during data migration)
- **closed**: Permanently closed (e.g., patient deceased, record archived)

**Properties:**

- Mutable, overwriteable
- Git-versioned for audit trail
- Uses FHIR-aligned wire format for interoperability
- Validated against strict schema with UUID checks

---

## Key Components

### Messaging Coordination

Manages clinical communication threads between clinicians, patients, and authorized participants.

See [Messaging Design](messaging.md) for detailed specifications.

### Encounter Management

Tracks patient encounters and episodes of care:

- Episode linkage and status
- Care team coordination
- Encounter documentation coordination

### Appointment Coordination

Manages appointment scheduling and coordination:

- Cross-system availability
- Resource allocation
- Cancellation and rescheduling coordination

---

## Design Principles

### Separation of Concerns

Coordination data is strictly separated from clinical content:

- **Clinical records** (EHR): What happened, what was said, what was observed
- **Coordination state**: Who needs to know, what needs to be done, system state

### Soft State

Coordination data is reconstructible and non-critical:

- Can be rebuilt from clinical records if lost
- Stale data causes inconvenience, not clinical harm
- Optimized for availability over consistency

### Cross-System Coordination

Enables seamless care delivery across multiple systems:

- Shared state for care teams
- Consistent patient experience
- Reduced administrative overhead

---

## Integration with VPR Components

### Relationship to Clinical Repository

- **Explicitly linked**: Each coordination record has a `clinical_id` in COORDINATION_STATUS.yaml
- **Initialization dependency**: Coordination records require an existing clinical record UUID
- **References not duplication**: Does not duplicate clinical content
- **Separation of concerns**: Clinical facts vs. coordination state
- **Enables coordination without coupling**: Systems can coordinate without accessing clinical details

### Relationship to Demographics

- Links coordination activities to patient identity
- Supports care team management
- Enables patient portal integration

### API Integration

- REST and gRPC APIs provide coordination services
- Separate from clinical record APIs
- Optimized for coordination workflows

---

## Lifecycle and Retention

Coordination data follows different retention policies than clinical records:

- **Short-term retention**: Active coordination state (weeks/months)
- **Medium-term retention**: Historical coordination for audit (years)
- **Long-term retention**: Minimal essential coordination metadata

Retention policies balance operational needs with privacy and storage costs.

---

## Future Extensions

The coordination repository provides foundation for:

- **Advanced workflow management**: Task assignment, delegation tracking
- **Multi-organisation coordination**: Cross-provider care coordination
- **Patient engagement**: Portal integration, preference management
- **Quality improvement**: Workflow analytics, performance metrics

---

## References

- [VPR Architecture Overview](../overview.md)
- [Clinical Repository Design](../design-decisions.md)
- [Messaging Design](messaging.md)
- [FHIR Integration](fhir.md)
- [API Specifications](../../specifications.md)
