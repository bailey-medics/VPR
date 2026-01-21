# Care Coordination Repository (CCR) Messaging – Design and Rationale

## Purpose

The CCR messaging system provides a **clinical, auditable, interoperable record of asynchronous communication** between clinicians, patients, and other authorised participants.

It is designed to:

- support cross-site and cross-system care coordination,
- remain human-readable without specialist software,
- withstand audit, legal, and regulatory review,
- avoid asserting certainty about human behaviour that the system cannot honestly know.

Messaging in the Care Coordination Repository is treated as **clinical communication**, not as a transient chat feature.

Conceptually, CCR messaging is **FHIR-aligned**, using the semantics of the FHIR Communication resource as a guiding model, without adopting FHIR storage formats or server behaviour.

---

## Conceptual model (FHIR-aligned)

Each CCR message corresponds conceptually to a **FHIR Communication**:

- it represents something that has already been communicated,
- it has an author, recipients, a timestamp, content, and a status,
- it is a clinical artefact with medico-legal weight.

CCR does not implement FHIR JSON, REST endpoints, or transport semantics. Instead, it preserves FHIR meaning while using a versioned, repository-based storage model aligned with the Versioned Patient Repository.

This guarantees that CCR messaging can be projected to FHIR Communication in future integrations, without constraining internal design.

---

## Core principles

### 1. Messaging is clinical

Messages exchanged between clinicians, patients, and other healthcare participants carry clinical and medico-legal weight equivalent to:

- written advice,
- clinic letters,
- documented telephone or video consultations.

As such, CCR messages form part of the clinical coordination record.

---

### 2. Messages are immutable

Once recorded, messages:

- MUST NOT be edited,
- MUST NOT be deleted.

This mirrors paper records, professional guidance, and legal expectations.

Errors or clarifications are handled via **corrections (addenda)**, never by modifying the original message.

---

### 3. Context matters more than individual messages

Individual messages often do not make sense in isolation.

For example:

> “Yes, I will do that doctor”

only has meaning when read alongside preceding and subsequent messages.

For this reason, the **conversation thread** is the meaningful clinical unit, not the individual message.

This aligns with FHIR Communication, which is frequently contextualised by related communications, encounters, or care plans.

---

## Repository placement

Messaging is a first-class concern of the **Care Coordination Repository (CCR)**.

It sits alongside other coordination artefacts (for example, tasks or referrals added later), and is explicitly separated from:

- clinical facts (clinical repository),
- demographics and identity (demographics repository).

---

## File layout

Each messaging thread is stored as:

```text
coordination/
    <shard1>/
        <shard2>/
            <coordination-uuid>/
                COORDINATION_STATUS.yaml
                communications/
                    <thread-id>/
                        messages.md
                        ledger.yaml
```

The coordination repository is sharded by UUID for scalability, similar to clinical records.

Where:

- `<thread-id>` is a timestamp-prefixed UUID (e.g., `20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000`)
- `messages.md` contains the canonical clinical conversation
- `ledger.yaml` contains thread metadata and participant information

---

### Thread identity

The `<thread-id>` is generated using a timestamp-prefixed identifier:

- format: `YYYYMMDDTHHMMSS.sssZ-UUID`
- timestamp: UTC, ISO 8601, millisecond precision
- UUID: randomly generated

Example:

```text
20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000
```

This ensures thread identifiers are:

- globally unique,
- chronologically sortable,
- suitable for distributed systems.

The existing `TimestampId` struct is used to generate and validate these identifiers.

---

## `messages.md` – Canonical clinical conversation

### Purpose

`messages.md` is the **canonical clinical record** of the conversation.

It records:

- what was communicated,
- by whom,
- when,
- and in what coordination context.

Conceptually, each entry corresponds to a FHIR Communication instance.

---

### Properties

- Append-only
- Immutable once written
- Human-readable
- Git-versioned
- Suitable for audit and legal review

---

### Message identity

Every message MUST include a globally unique `message_id` (UUID).

Message identifiers exist to:

- unambiguously identify messages,
- allow corrections to reference prior messages,
- support projections, caches, and alert suppression.

Timestamps are used for ordering, not identity.

---

### Message types

`messages.md` may contain:

- clinician messages
- patient messages
- system messages
- correction messages

System messages (for example, “participant added to thread”) are first-class entries, as they provide clinically and legally relevant coordination context.

---

### Corrections (addenda)

Errors or clarifications are recorded as **new messages**, not edits.

A correction message:

- is a new message,
- has its own `message_id`,
- references the original message via `corrects: <message_id>`.

The original message is never modified.

This preserves a truthful, auditable historical record.

---

### Explicit non-features

`messages.md` does NOT record:

- read or seen status,
- urgency flags,
- acknowledgement or acceptance,
- task completion or responsibility transfer.

These concepts imply human cognition or behaviour that the system cannot verify and therefore does not assert.

---

### Example structure

```markdown
# Messages

## Message

**ID:** `3f7a8d2c-1e9b-4a6d-9f2e-5c8b7a4d1f92`  
**Type:** clinician  
**Timestamp:** 2026-01-11T14:36:15.234Z  
**Author ID:** `4f8c2a1d-9e3b-4a7c-8f1e-6b0d2c5a9f12`  
**Author:** Dr Jane Smith

Patient has reported increasing shortness of breath.
Please review chest X-ray and advise on next steps.

---

## Message

**ID:** `8b2f6a5c-3d1e-4a9b-8c7f-6d5e4a3b2c1d`  
**Type:** clinician  
**Timestamp:** 2026-01-11T15:42:30.567Z  
**Author ID:** `a1d3c5e7-f9b2-4680-b2d4-f6e8c0a9d1e3`  
**Author:** Dr Tom Patel

Reviewed X-ray. No acute changes. Continue current management
and reassess in 48 hours. If symptoms worsen, arrange urgent review.
```

---

## `ledger.yaml` – Thread context and policy

### Purpose

`ledger.yaml` stores **contextual and policy metadata**, not clinical narrative.

It answers:

> “Who is involved in this conversation, and under what rules?”

---

### Typical contents

- participants and roles
- visibility and sensitivity flags
- thread status (open, closed, archived)
- organisational access rules

```yaml
thread_id: 20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000

status: open
created_at: 2026-01-11T14:35:22.045Z
last_updated_at: 2026-01-11T15:10:04.912Z

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
  allow_external_organisations: true
```

---

### Properties

- Mutable
- Overwriteable
- Git-audited
- Changes are deliberate and relatively infrequent
- `last_updated_at` is automatically updated when messages are added

**Thread-level metadata:**

- Thread status: open, closed, or archived
- Participant list with roles (organisation field removed for simplicity)
- Visibility and sensitivity settings
- Participation policies (external organisations allowed by default)

**Audit trail:** Inherent in Git commit history and messages.md content - no separate audit section needed.

---

### Explicit exclusions

`ledger.yaml` does NOT contain:

- message content,
- interaction or navigation state,
- user interface hints.

---

## Git Versioning

All changes to coordination records are Git-versioned for audit purposes:

```text
coordination:create: Created messaging thread

Care-Location: Oxford University Hospitals
```

```text
coordination:update: Added message to thread

Care-Location: Oxford University Hospitals
```

```text
coordination:update: Updated thread participant list

Care-Location: Oxford University Hospitals
```

Commits include:

- Structured commit messages with domain and action
- Care location metadata
- Optional cryptographic signatures
- Full audit trail of all changes

---

## Alerting behaviour

CCR does **not** record:

- Read receipts or "seen" status
- Acknowledgements
- Urgency flags
- Task completion or responsibility transfer

These concepts imply human cognition or behaviour that the system cannot verify.

Consuming systems may implement alerting by:

1. Tracking their own render/presentation state externally (not in VPR)
2. Comparing message timestamps to their last-viewed records
3. Presenting unread indicators in their user interface

This approach:

- Avoids false certainty about human understanding
- Reduces legal and clinical ambiguity
- Maintains truthful audit trails
- Enables consistent patient experience across systems

Alerting is a **user-experience concern**, not a clinical record.

---

## Thread Lifecycle

Messaging threads follow a defined lifecycle:

### Creation

Threads are created via `CoordinationService::create_thread()`:

- Generates timestamp-prefixed thread ID
- Creates `communications/<thread-id>/` directory
- Writes initial `messages.md` (optionally with first message)
- Writes `ledger.yaml` with participant list and policies
- Commits atomically to Git

### Message Addition

Messages are added via `CoordinationService::add_message()`:

- Generates unique message UUID
- Appends to `messages.md` (preserves immutability)
- Commits with structured message and care location
- Returns the message ID for reference

### Metadata Updates

Thread metadata is updated via `CoordinationService::update_thread_ledger()`:

- Modifies `ledger.yaml` (participants, status, policies)
- Git commit records the change
- Audit log tracks all modifications

### Status Transitions

Threads can transition between states:

- **Open** → **Closed**: Thread completed, no new messages accepted
- **Closed** → **Archived**: Thread moved to archive, hidden from default views
- **Open** → **Archived**: Direct archival without closing

### Deletion

Threads are **never deleted**:

- Immutability is preserved
- Audit trail remains complete
- Archival is used instead of deletion
- Git history retains full record

---

---

## Implementation Details

### Initialization

Coordination repositories are initialized with:

```rust
CoordinationService::new(cfg)
    .initialise(author, care_location, clinical_id)
```

This creates:

- Sharded directory structure: `coordination/<s1>/<s2>/<uuid>/`
- `COORDINATION_STATUS.yaml` with link to clinical record
- Git repository with initial commit
- Lifecycle state set to `active`

### Thread Creation

Messaging threads are created with:

```rust
service.create_thread(
    &author,
    care_location,
    participants,
    initial_message
)
```

This:

- Generates timestamp-prefixed thread ID via `TimestampIdGenerator`
- Creates `communications/<thread-id>/` directory
- Writes `messages.md` with optional initial message
- Writes `ledger.yaml` with participant list and policies
- Commits both files atomically to Git

### Adding Messages

Messages are appended with:

```rust
service.add_message(
    &author,
    care_location,
    thread_id,
    message_content
)
```

This:

- Generates unique message UUID
- Appends to `messages.md` (preserves immutability)
- Commits with structured message and care location
- Returns the message ID

### Type Safety

The `CoordinationService` uses type-state pattern:

- `CoordinationService<Uninitialised>` - Can only call `initialise()`
- `CoordinationService<Initialised>` - Can call thread and message operations

This prevents operations on non-existent repositories at compile time.

### Error Handling

Operations return `PatientResult<T>` with comprehensive error types:

- Author validation errors
- Git operation failures
- File I/O errors
- FHIR wire format validation errors
- UUID parsing errors

Cleanup is attempted on initialization failure to prevent partial repositories.

---

## Design decisions explicitly rejected

The following were deliberately excluded:

- read receipts (opening does not equal reading or understanding)
- urgency flags (asynchronous messaging is not suitable for urgent care)
- acknowledgement tracking (implies responsibility transfer)
- workflow or task semantics (these may be added later using FHIR-aligned Task concepts)

These exclusions reduce legal ambiguity, false certainty, and unintended clinical inference.

---

## References

- [Coordination Index](index.md)
- [FHIR Integration](fhir.md)
- [FHIR Communication Resource](https://hl7.org/fhir/communication.html)
- [VPR Architecture Overview](../overview.md)
