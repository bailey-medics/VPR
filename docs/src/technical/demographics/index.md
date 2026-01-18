# Demographics Repository

## 1. Purpose

The Demographics Repository is responsible for storing and managing **patient identity and demographic information** within VPR.

Its primary purpose is to provide a clear, authoritative, and interoperable representation of _who the patient is_, distinct from:

- what care they have received,
- what clinical observations have been recorded,
- and how care is coordinated.

Demographic data is foundational. Errors in demographics propagate risk across all other systems. For this reason, the Demographics Repository is deliberately separated from clinical and care coordination data.

---

## 2. Scope

The Demographics Repository contains **identity and demographic information only**. This includes, but is not limited to:

- Names and aliases
- Date of birth
- Sex and gender-related attributes
- Addresses and contact details
- Identifiers (NHS number, local identifiers)
- Deceased status
- Links to related persons where appropriate

It does **not** contain:

- clinical observations,
- diagnoses,
- procedures,
- correspondence content,
- care plans or workflows.

---

## 3. Use of FHIR

VPR uses **FHIR** (Fast Healthcare Interoperability Resources) as the canonical model for demographic data.

FHIR is used because it:

- is widely adopted across healthcare systems,
- has a clear and extensible Patient model,
- supports interoperability with existing NHS and international systems,
- cleanly separates identity from clinical content.

FHIR resources are stored and handled in a way that preserves their structure and semantics.

---

## 4. Primary FHIR Resource

### 4.1 Patient Resource

The core resource used in the Demographics Repository is the **FHIR Patient** resource.

The Patient resource represents:

- a single individual receiving or potentially receiving care,
- with zero or more identifiers,
- and zero or more contact and demographic attributes.

Only attributes relevant to identity and demographics are populated.

---

## 5. Separation from Clinical Repositories

The Demographics Repository is intentionally separate from the Clinical Repository.

Key reasons for this separation include:

- Demographic data changes more frequently and independently.
- Identity errors require different correction and governance processes.
- Many systems need demographic access without clinical access.
- Clinical data must not be invalidated by demographic corrections.

Clinical records reference patients by identifier rather than embedding demographic fields.

---

## 6. Corrections and Redactions

Demographic errors can have serious consequences.

When demographic information is determined to be incorrect or misattributed:

- Corrections are made by updating or superseding the relevant FHIR resource.
- Redacted demographic artefacts are moved to the **Redaction Retention Repository (RRR)**.
- A reference remains to indicate that a correction has occurred.

Demographic information is never silently deleted.

---

## 7. Versioning and Change History

Demographic changes are expected and supported.

The Demographics Repository maintains:

- a full history of changes,
- attribution of who made each change,
- timestamps and reason codes where available.

This supports traceability, auditability, and patient safety.

---

## 8. Access and Authorisation

Access to demographic data is role-based and purpose-limited.

Different roles may have:

- read-only access,
- update access,
- linkage access for cross-system identity resolution.

Demographic access does not imply access to clinical content.

---

## 9. Relationship to Other VPR Components

- Clinical Repository: references patients by identifier only.
- Care Coordination Repository: links to patient identity without duplicating demographics.
- Redaction Retention Repository: stores superseded or misattributed demographic artefacts.
- External systems: demographic data may be exchanged using FHIR interfaces.

---

## 10. Design Principles

- Identity before care
- Correction without erasure
- Interoperability by default
- Clear separation of concerns
- Auditability without friction

---

## 11. Summary

The Demographics Repository provides a stable, interoperable, and auditable foundation for patient identity within VPR.

By using FHIR and maintaining strict separation from clinical and care coordination data, VPR ensures that identity errors can be corrected safely without compromising the integrity of the clinical record.
