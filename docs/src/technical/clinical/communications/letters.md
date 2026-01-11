# VPR Letters – Design and Rationale

## Purpose

The VPR letters system provides a **clinical, auditable, interoperable record of formal written correspondence** related to patient care.

It is designed to:

- support cross-site and cross-system communication,
- remain human-readable without specialist software,
- withstand audit, legal, and regulatory review.

Letters in VPR are treated as **formal clinical documents**, not drafts, messages, or workflow artefacts.

---

## Letters are immutable once issued

Once a letter is issued, it:

- MUST NOT be edited,
- MUST NOT be deleted.

If a correction or clarification is required, this is handled by issuing a **new letter** that explicitly references the prior one.

This mirrors established professional, legal, and regulatory practice.

> **Note on “issued”**  
> A letter is considered *issued* when it is finalised and made available outside the authoring context (for example, shared with a patient, sent to another organisation, or rendered as a PDF for distribution). Drafts that have not been issued are out of scope for this model.

---

## File layout

Each letter is stored as a self-contained folder:

```text
communications/
    letters/
        <letter-id>/
            letter.md
            comments.md
            ledger.yaml
            letter.pdf   # via Git LFS pointer if large
```

This structure ensures that all artefacts related to a single letter are co-located and auditable.

---

## Letter identity

The `<letter-id>` is generated in the format `YYYYMMDDTHHMMSS.sssZ-UUID`:

- `YYYYMMDDTHHMMSS.sssZ` – ISO 8601 timestamp with millisecond precision
- `UUID` – Randomly generated UUID v4, without hyphens
- Example:  
  `20260111T143522.045Z-550e8400e29b41d4a716446655440000`

This ensures letters are:

- globally unique,
- chronologically sortable,
- safe for distributed and concurrent systems.

---

## `letter.md` – Canonical clinical letter

### Purpose

`letter.md` is the **canonical clinical representation** of the letter.

It records:

- the full letter content,
- authorship,
- recipients,
- date of issue,
- clinical intent.

---

### Properties

- Immutable once written
- Human-readable Markdown
- Git-versioned
- Suitable for audit, legal review, and patient access

---

### Required structure (conceptual)

A letter SHOULD clearly contain:

- header information (author, organisation, date),
- recipient(s),
- subject or reason for correspondence,
- clinical narrative,
- actions or recommendations (if any),
- signature block.

The exact formatting is intentionally flexible to accommodate different clinical contexts.

---

### Letter identity (internal)

Every letter MUST include a globally unique `letter_id` (UUID, without hyphens), recorded within the document.

Letter IDs exist to:

- unambiguously reference letters,
- allow later letters to reference earlier correspondence,
- support indexing and cross-system linkage.

Timestamps provide chronology, not identity.

---

### Corrections and follow-up

Errors or clarifications are handled by issuing a **new letter**.

A corrective letter:

- is a new clinical document,
- has its own `letter_id`,
- references the prior letter via `references: <letter_id>`.

The original letter is never modified.

This preserves an honest and legally defensible historical record.

---

### Explicit non-features

`letter.md` does NOT record:

- read or opened status,
- acknowledgement,
- urgency markers,
- task or workflow state.

Letters represent communication, not behaviour.

---

## `comments.md` – Authored statements related to the letter

### Purpose

`comments.md` contains **append-only authored statements** related to the letter.

Comments may be authored by:

- patients,
- relatives or carers,
- clinicians.

These statements are part of the patient record but **do not modify, correct, or supersede the letter**.

---

### Properties

- Append-only
- Each entry has explicit authorship and timestamp
- Human-readable Markdown
- Git-versioned

If a comment is incorrect or withdrawn, this is handled by adding a new comment, not editing an existing one.

---

## `ledger.yaml` – Letter context and policy

### Purpose

`ledger.yaml` stores **contextual and policy metadata**, not clinical prose.

It answers:

> “Who issued this letter, to whom, and under what rules?”

---

### Typical contents

- author and issuing organisation,
- intended recipients and roles,
- letter type (clinic, discharge, referral, advice),
- sensitivity and visibility flags,
- access control rules,
- status (draft, issued, superseded),
- lineage information (for example, supersedes).

---

### Properties

- Mutable
- Overwriteable
- Git-audited
- Changes are intentional and relatively infrequent

---

### Explicit exclusions

`ledger.yaml` does NOT contain:

- letter narrative content,
- editing history of prose,
- UX or presentation hints.

---

## Large binary artefacts

Large binary artefacts (for example PDFs with embedded images or scans) are stored using **Git Large File Storage (Git LFS)**.

In practice this means:

- a small pointer file is stored in the Git repository,
- the binary content is stored in a separate object store,
- the pointer is versioned, immutable, and content-addressed.

From a clinical and audit perspective, these artefacts are first-class parts of the letter record.

---

## Supersession and lineage

Later letters may **supersede** earlier ones.

Supersession:

- is declared explicitly in `ledger.yaml`,
- never alters the original letter,
- allows consuming systems to identify the most current guidance.

Historical letters remain valid records of what was communicated at the time.

---

## Design decisions explicitly rejected

The following were deliberately excluded:

- inline editing of letters after issue,
- read receipts or confirmations,
- urgency flags,
- task or workflow semantics.

These features introduce legal ambiguity and false certainty.

VPR letters prioritise **clarity, honesty, and auditability** over convenience.
