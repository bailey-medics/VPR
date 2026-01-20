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
    communications/
        <thread-id>/
            messages.md
            ledger.yaml
```

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
  - participant_id: 4f8c2a1d9e3b4a7c8f1e6b0d2c5a9f12
    role: clinician
    display_name: Dr Jane Smith
    organisation: Gloucestershire Hospitals NHS Foundation Trust

  - participant_id: a1d3c5e7f9b24680b2d4f6e8c0a9d1e3
    role: clinician
    display_name: Dr Tom Patel
    organisation: Gloucestershire Hospitals NHS Foundation Trust

  - participant_id: 9b7c6d5e4f3a2b1c0e8d7f6a5b4c3d2
    role: patient
    display_name: John Doe
    organisation: null

visibility:
  sensitivity: standard
  restricted: false

policies:
  allow_patient_participation: true
  allow_external_organisations: false

audit:
  created_by: system
  change_log:
    - changed_at: 2026-01-11T14:35:22.045Z
      changed_by: system
      description: Thread created
```

---

### Properties

- Mutable
- Overwriteable
- Git-audited
- Changes are deliberate and relatively infrequent

---

### Explicit exclusions

`ledger.yaml` does NOT contain:

- message content,
- interaction or navigation state,
- user interface hints.

---

## Alerting behaviour

CCR does not record alerts as clinical facts.

Consuming systems may derive alerts by:

1. Reading the latest `message_id` from `messages.md`
2. Comparing it to a participant-specific render cursor (stored externally or in a coordination projection)
3. Alerting only if messages exist beyond the last render attempt

Alerting is a user-experience concern, not a clinical record.

---

## Design decisions explicitly rejected

The following were deliberately excluded:

- read receipts (opening does not equal reading or understanding)
- urgency flags (asynchronous messaging is not suitable for urgent care)
- acknowledgement tracking (implies responsibility transfer)
- workflow or task semantics (these may be added later using FHIR-aligned Task concepts)

These exclusions reduce legal ambiguity, false certainty, and unintended clinical inference.
