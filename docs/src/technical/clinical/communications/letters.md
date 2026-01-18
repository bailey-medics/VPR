# VPR Letters – Design and Rationale

## Purpose

The VPR letters system provides a **clinical, auditable, interoperable record of formal written correspondence** related to patient care.

It is designed to:

- support cross-site and cross-system communication,
- remain human-readable without specialist software,
- withstand audit, legal, and regulatory review.

This document intentionally avoids imposing stylistic rules on how letters are written. Clinical correspondence varies widely by specialty, country, organisation, and individual clinician. VPR preserves this freedom while enabling safe reuse of selected clinical context.

---

## Letters are version-controlled via git

Letters can be edited, with all changes tracked through git version control. OpenEHR does not specify that letters must be closed to further edits.

This means:

- Every edit creates a new git commit,
- The full history of changes is preserved and auditable,
- Previous versions can be retrieved at any time.

This provides both flexibility and a complete audit trail.

---

## File layout

Each letter is stored as a self-contained folder:

```text
correspondence/
    letter/
        <letter-id>/
            composition.yaml
            body.md
            attachments/
                letter.pdf
```

This structure ensures that all artefacts related to a single letter are co-located, versioned, and auditable.

---

## Letter identity

The `<letter-id>` is generated in the format:

```text
YYYYMMDDTHHMMSS.sssZ-UUID
```

- `YYYYMMDDTHHMMSS.sssZ` – timestamp with millisecond precision
- `UUID` – random UUID v4 in RFC 4122 format (lowercase with hyphens)

Example:

```text
20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000
```

This ensures letters are:

- globally unique,
- chronologically sortable within a patient record,
- safe for distributed, batch-based, and concurrent systems.

Timestamps provide chronology, not global ordering guarantees.

---

## `composition.yaml` – OpenEHR composition

The `composition.yaml` file contains the **OpenEHR-aligned COMPOSITION envelope** for the letter.

It captures:

- identity
- authorship
- time context
- semantic intent
- structured, reusable clinical snapshots (optional)

### Example `composition.yaml`

```yaml
rm_version: "rm_1_1_0"
uid: "20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000"

archetype_node_id: "openEHR-EHR-COMPOSITION.correspondence.v1"

name:
  value: "Clinical letter"

category:
  value: "event"

composer:
  name: "Dr Jane Smith"
  role: "Consultant Physician"

context:
  start_time: "2026-01-12T10:14:00Z"

content:
  - section:
      archetype_node_id: "openEHR-EHR-SECTION.correspondence.v1"
      name:
        value: "Correspondence"

      items:
        # Canonical narrative letter
        - evaluation:
            archetype_node_id: "openEHR-EHR-EVALUATION.clinical_correspondence.v1"
            name:
              value: "Clinical correspondence"

            data:
              narrative:
                type: "external_text"
                path: "./body.md"

        # Optional reusable snapshot entries
        - evaluation:
            archetype_node_id: "openEHR-EHR-EVALUATION.snapshot.v1"
            name:
              value: "Diagnoses (snapshot)"

            data:
              kind:
                value: "diagnoses"

              items:
                - text: "Hypertension"
                  code:
                    terminology: "SNOMED-CT"
                    value: "38341003"

                - text: "Hyperlipidaemia"

                - text: "Chronic obstructive pulmonary disease"
                  code:
                    terminology: "SNOMED-CT"
                    value: "13645005"

        - evaluation:
            archetype_node_id: "openEHR-EHR-EVALUATION.snapshot.v1"
            name:
              value: "Medication summary (snapshot)"

            data:
              kind:
                value: "medications"

              items:
                - text: "Amlodipine 10 mg once daily"
                
                - text: "Atorvastatin 20 mg nocte"
```

### Notes

- `openEHR-EHR-EVALUATION.snapshot.v1` is a **custom archetype**, not a core OpenEHR entity.
- This is intentional and aligned with OpenEHR practice.
- Snapshots are **letter-scoped, time-bound clinical summaries**, not canonical state.

---

## Snapshot EVALUATION – design intent

The `snapshot.v1` archetype is intentionally **minimal and generic**.

Its purpose is to support selective reuse of clinically relevant context *without* enforcing letter style or duplicating persistent records.

### Snapshot properties

Each snapshot EVALUATION:

- represents **one kind of reusable clinical context**,
- is explicitly scoped to **this letter only**,
- may be copied forward by user choice,
- makes **no claim of completeness or authority**.

### Minimal conceptual model

A snapshot contains:

- `kind` – a semantic label identifying what this snapshot represents  
  (for example: `diagnoses`, `medications`, `social_history`, `functional_status`)
- `items` – zero or more entries
- optional narrative text (when structure is insufficient)

The set of possible `kind` values is **open-ended**. VPR does not enforce an enum.

Unknown kinds are valid and must degrade gracefully.

---

## Coded and uncoded items

Snapshot items may be:

- **coded**,
- **uncoded**, or
- **mixed within the same snapshot**.

Coding is optional and must never be required.

### Example

```yaml
items:
  - text: "Hypertension"
    code:
      terminology: "SNOMED-CT"
      value: "38341003"

  - text: "Lives alone, independent"
```

This supports real-world clinical practice where:

- some concepts are well-coded,
- others are contextual or narrative,
- and forcing codes would lose meaning.

---

## Relationship to persistent clinical lists

Snapshots are **not** persistent lists.

They answer a different question:

- Persistent list: *"What do we currently believe is true?"*
- Snapshot: *"What did the author consider relevant for this letter at that time?"*

Snapshots:

- may omit persistent items,
- may include provisional information,
- may differ between letters,
- must never automatically update canonical state.

Reconciliation occurs only through explicit clinical action and new COMPOSITIONs.

---

## `body.md` – Canonical clinical letter

### Purpose

`body.md` contains the **canonical narrative letter**.

It records:

- clinical prose only,
- written for human readers,
- frozen at the time of issue.

It must not contain workflow, delivery, or coordination semantics.

### Example `body.md`

```markdown
Dear Dr Patel,

Thank you for seeing Mrs Jane Jones (DOB 12/04/1968) in the respiratory clinic today.

She reports an improvement in breathlessness since her last review. She confirms that she is currently taking amlodipine 10 mg once daily, rather than the previously documented dose of 5 mg.

We reviewed her medication list together. Atorvastatin was started during her recent admission. The intended dose is 20 mg nocte.

There are no new red flag symptoms. Examination today was unremarkable.

Plan:
- Continue amlodipine 10 mg once daily
- Continue atorvastatin 20 mg nocte
- Routine follow-up in six months

Kind regards,

Dr Jane Smith  
Consultant Respiratory Physician  
Example NHS Trust
```

### Properties

- Editable after issue, with full git version history
- Human-readable Markdown
- Git-versioned with complete audit trail
- Suitable for audit, legal review, and patient access

---

## Large binary artefacts

Large binary artefacts (for example PDFs with embedded images or scans) are stored using Git Large File Storage (Git LFS).

This means:

- a small pointer file is stored in the Git repository,
- binary content is stored in an external object store,
- pointers are versioned, immutable, and content-addressed.

From a clinical and audit perspective, these artefacts are first-class parts of the letter record.

---

## Explicit non-features

The following are deliberately excluded from the letter model:

- read or opened status
- acknowledgements
- urgency markers
- task or workflow state

Letters represent **clinical documentation**, not behaviour or process.

VPR prioritises clarity, honesty, and auditability over convenience.

- support cross-site and cross-system communication,
- remain human-readable without specialist software,
- withstand audit, legal, and regulatory review.

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
correspondence/
    letter/
        <letter-id>/
            composition.yaml
            body.md
            attachments/
                letter.pdf
```

This structure ensures that all artefacts related to a single letter are co-located and auditable.

---

## Letter identity

The `<letter-id>` is generated in the format `YYYYMMDDTHHMMSS.sssZ-UUID`:

- `YYYYMMDDTHHMMSS.sssZ` – ISO 8601 timestamp with millisecond precision
- `UUID` – Randomly generated UUID v4 in RFC 4122 format (lowercase with hyphens)
- Example:  
  `20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000`

This ensures letters are:

- globally unique,
- chronologically sortable,
- safe for distributed and concurrent systems.

---

## `composition.yaml` – OpenEHR composition

The `composition.yaml` file contains the **OpenEHR composition** representing the letter's metadata and structure, as below:

```yaml
rm_version: "1.0.4" # updatable via api
uid: "20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000"  # updatable via api
archetype_node_id: "openEHR-EHR-COMPOSITION.correspondence.v1"
name:
  value: "Clinical letter"
category:
  value: "event"
composer:
  name: "Dr Jane Smith"  # updatable via api
  role: "Consultant Physician"  # updatable via api
context:
  start_time: "2026-01-12T10:14:00Z"  # updatable via api
content:
  - section:
      archetype_node_id: "openEHR-EHR-SECTION.correspondence.v1"
      name:
        value: "Correspondence"
      items:
        - evaluation:
            archetype_node_id: "openEHR-EHR-EVALUATION.clinical_correspondence.v1"
            name:
              value: "Clinical correspondence"

            data:
              narrative:
                type: "external_text"
                path: "./body.md"
        - evaluation:
            archetype_node_id: "openEHR-EHR-EVALUATION.problem_summary.v1"
            name:
              value: "Diagnoses at time of correspondence"

            data:
              diagnoses:
                - name: "Hypertension"
                - name: "Hyperlipidaemia"
                - name: "Chronic obstructive pulmonary disease"
```

NB: `# updatable via api` is placed to indicate fields that may be modified by the OpenEHR API.

---

## `body.md` – Canonical clinical letter

### Purpose

`body.md` is the **canonical clinical representation** of the letter.
It records:

- the full letter content only

An example of `body.md` might look like:

```markdown
Dear Dr Patel,

Thank you for seeing Mrs Jane Jones (DOB 12/04/1968) in the respiratory clinic today.

She reports an improvement in breathlessness since her last review. She confirms that she is currently taking amlodipine 10 mg once daily, rather than the previously documented dose of 5 mg.

We reviewed her medication list together. Atorvastatin was started during her recent admission. The intended dose is 20 mg nocte.

There are no new red flag symptoms. Examination today was unremarkable.

Plan:
- Continue amlodipine 10 mg once daily
- Continue atorvastatin 20 mg nocte
- Routine follow-up in six months

Kind regards,

Dr Jane Smith  
Consultant Respiratory Physician  
Example NHS Trust
```

---

### Properties

- Editable after issue, with full git version history
- Human-readable Markdown with front matter metadata
- Git-versioned with complete audit trail
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

Every letter MUST include a globally unique `letter_id` (RFC 4122 UUID with hyphens), recorded within the document.

Letter IDs exist to:

- unambiguously reference letters,
- allow later letters to reference earlier correspondence,
- support indexing and cross-system linkage.

Timestamps provide chronology, not identity.

---

### Corrections and follow-up

Errors or clarifications may be handled either by:

1. **Editing the existing letter** (with git tracking all changes), or
2. **Issuing a new letter** that references the prior one via `references: <letter_id>`.

Both approaches are valid. Git version control preserves an honest and legally defensible historical record of all changes.

---

### Explicit non-features

`letter.md` does NOT record:

- read or opened status,
- acknowledgement,
- urgency markers,
- task or workflow state.

Letters represent communication, not behaviour.

---

## `comments.md`

See [Comments section](../../comments.md) for details.

---

## Large binary artefacts

Large binary artefacts (for example PDFs with embedded images or scans) are stored using c.

In practice this means:

- a small pointer file is stored in the Git repository,
- the binary content is stored in a separate object store,
- the pointer is versioned, immutable, and content-addressed.

From a clinical and audit perspective, these artefacts are first-class parts of the letter record.

---

## Design decisions explicitly rejected

The following were deliberately excluded:

- read receipts or confirmations,
- urgency flags,
- task or workflow semantics.

These features introduce legal ambiguity and false certainty.

VPR letters prioritise **clarity, honesty, and auditability** over convenience.
