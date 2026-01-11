# VPR Messaging – Design and Rationale

## Purpose 1

The VPR messaging system provides a **clinical, auditable, interoperable record of asynchronous communication** between clinicians, patients, and other authorised participants.

It is designed to:

- support cross-site and cross-system care,
- remain human-readable without specialist software,
- withstand audit and legal review,
- avoid asserting certainty about human behaviour that the system cannot honestly know.

Messaging in VPR is treated as **clinical communication**, not as a transient chat feature.

---

## Core principles

### 1. Messaging is clinical

Messages exchanged between clinicians and patients carry clinical and medico-legal weight equivalent to:

- written advice,
- clinic letters,
- documented telephone consultations.

As such, messages are part of the clinical record.

---

### 2. Messages are immutable

Once recorded, messages:

- MUST NOT be edited,
- MUST NOT be deleted.

This mirrors paper records and professional guidance.

Errors or clarifications are handled via **corrections (addenda)**, not edits.

---

### 3. Context matters more than individual messages

Individual messages often do not make sense in isolation.

For example:
> “Yes, I will do that doctor”

only has meaning when read alongside preceding and subsequent messages.

For this reason, **the conversation thread is the meaningful clinical unit**, not the individual message.

---

## File layout

Each messaging thread is stored as:

```text
communications/
    messages/
        <thread-id>/
            messages.md
            ledger.yaml
```

### Thread Identity

The `<thread-id>` is generated in the format `YYYYMMDDTHHMMSS.sssZ-UUID`:

- `YYYYMMDDTHHMMSS.sssZ`: ISO 8601 timestamp with millisecond precision (e.g., `20260111T120000.123Z`)
- `UUID`: Randomly generated UUID v4
- Example: `20260111T120000.123Z-550e8400-e29b-41d4-a716-446655440000`

This ensures thread IDs are globally unique and chronologically sortable.

---

## `messages.md` – Canonical clinical conversation

### Purpose 2

`messages.md` is the **canonical clinical record** of the conversation.

It records:

- what was communicated,
- by whom,
- when,
- and in what context.

### Properties 1

- Append-only
- Immutable once written
- Human-readable
- Git-versioned
- Suitable for audit and legal review

### Message identity

Every message MUST include a globally unique `message_id` (UUID).

Message IDs exist to:

- unambiguously identify messages,
- allow corrections to reference prior messages,
- support projections, caches, and alert suppression.

Timestamps are used for ordering, not identity.

---

### Message types

`messages.md` may contain:

- **Clinician messages**
- **Patient messages**
- **System messages**
- **Correction messages**

System messages (for example, “participant added to thread”) are first-class entries, as they provide clinically and legally relevant context.

---

### Corrections (addenda)

Errors or clarifications are recorded as **new messages**, not edits.

A correction message:

- is a new message,
- has its own `message_id`,
- references the original message via `corrects: <message_id>`.

The original message is never modified.

This preserves a truthful historical record.

---

### Explicit non-features

`messages.md` does NOT record:

- read or seen status,
- urgency flags,
- acknowledgement,
- task completion,
- responsibility transfer.

These concepts imply human cognition or behaviour that the system cannot verify.

---

## `ledger.yaml` – Thread context and policy

### Purpose 3

`ledger.yaml` stores **contextual and policy metadata**, not clinical narrative.

It answers:
> “Who is involved in this conversation, and under what rules?”

### Typical contents

- Participants and roles
- Visibility and sensitivity flags
- Thread status (open, closed, archived)
- Organisational access rules

### Properties 2

- Mutable
- Overwriteable
- Git-audited
- Changes are deliberate and relatively infrequent

### Explicit exclusions

`ledger.yaml` does NOT contain:

- message content,
- interaction or navigation state,
- UX hints.

---

### Alerting behaviour

VPR does not record alerts.

Consuming systems can derive alerts by:

1. Reading the latest message_id from messages.md
2. Comparing it to the participant's `render_cursor` (stored in the `care coordination repo`), or their own tracking state
3. Alerting only if messages exist beyond the last render attempt

Alerting is advisory UX behaviour, not a clinical fact.

---

### Design decisions explicitly rejected

The following were deliberately excluded:

- Read receipts (opening does not equal reading or understanding)
- Urgency flags (asynchronous messaging is not suitable for urgent care)
- Acknowledgement tracking (implies responsibility transfer)
- Workflow or task semantics (VPR is not a task engine)

These exclusions reduce legal ambiguity and false certainty.
