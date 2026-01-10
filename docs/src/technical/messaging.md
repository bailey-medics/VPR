# VPR Messaging – Design and Rationale

## Purpose

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
