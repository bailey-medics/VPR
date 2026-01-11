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
          messaging/
            <thread-id>/
              render_state.yaml
          encounters/
            ...
          appointments/
            ...
```

---

## Key Components

### Messaging Coordination

Manages render state for clinical messaging threads to enable cross-system coordination.

#### File Layout

Render state is stored as:

```
coordination/
  messaging/
    <thread-id>/
      render_state.yaml
```

Where `<thread-id>` matches the unique identifier for each messaging thread in the EHR.

#### render_state.yaml Structure

The `render_state.yaml` file stores **best-effort system render coordination state**.

**Key Concept - render_cursor:**

> The furthest message ID that a consuming system last attempted to render when presenting the conversation.

This records **system behaviour**, not human behaviour. It does NOT imply that a participant read, saw, understood, acknowledged, or acted upon any message.

**Example structure:**

```yaml
participants:
  gmc:1234567:
    render_cursor: 6f2c1b8a-4f7c-4f7a-9e6b-3b3d7c8c6d92
    recorded_at: 2026-01-06T09:12:00Z
    source: quill-ehr-oxford
  patient:456:
    render_cursor: 6f2c1b8a-4f7c-4f7a-9e6b-3b3d7c8c6d92
    recorded_at: 2026-01-06T09:15:00Z
    source: patient-portal-oxford
```

**Properties:**

- Optional, mutable, overwriteable
- Soft state - allowed to be stale or wrong
- Losing it causes annoyance, not clinical harm
- Can be rebuilt from EHR message history

#### Git Versioning

Changes are Git-versioned for audit purposes:

```text
coordination:update: Render cursor updated for participant gmc:1234567

Care-Location: Oxford University Hospitals
```

Commits are frequent and automated, with signatures applied where configured.

#### Alerting Behaviour

VPR does not record alerts. Consuming systems derive alerts by:

1. Reading the latest message_id from EHR messages.md
2. Reading participant's render_cursor from coordination render_state.yaml
3. Alerting only if messages exist beyond the last render attempt

This enables:

- **Alert suppression**: Prevents duplicate notifications across systems
- **Continuity of care**: Consistent state across different EHR systems
- **Patient experience**: No redundant notifications

#### Lifecycle

Render state is created when participants first interact with threads and updated automatically by consuming systems. It may become stale if systems are offline but can always be rebuilt from EHR message history.

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

- References clinical records by UUID
- Does not duplicate clinical content
- Enables coordination without coupling

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
- [API Specifications](../../specifications.md)
