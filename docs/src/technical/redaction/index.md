# Redaction Retention Repository (RRR)

## 1. Purpose

The Redaction Retention Repository (RRR) exists to ensure that patient-related information which has been removed from routine views is **retained permanently, safely, and transparently**.

RRR supports correctness, accountability, and trust in the VPR system by ensuring that no information is silently lost, while also ensuring that routine clinical, demographic, and care coordination views remain accurate and appropriate for day-to-day use.

---

## 2. Scope

The RRR applies to **all patient-related information** managed by VPR, including but not limited to:

- Clinical entries
- Demographic data
- Care coordination artefacts
- Referrals and correspondence
- Attachments and structured documents

RRR is **not limited to clinical data** and is **not patient-owned**.

---

## 3. Core Principles

### 3.1 Retention, Not Deletion

Information placed into the RRR is never deleted. Retention is the default and permanent state unless explicitly governed by external policy or law.

### 3.2 Removal from Routine View

Items in the RRR must not appear in routine clinical or operational workflows. Their removal prevents inappropriate use while preserving traceability.

### 3.3 Neutrality

Placement into the RRR does not imply error, blame, review, or wrongdoing. It reflects a change in suitability for routine display only.

### 3.4 Transparency and Auditability

All movements into the RRR are recorded, attributable, and inspectable by authorised roles.

---

## 4. What “Redaction” Means in VPR

In VPR, **redaction** means:

> Removal of an artefact from routine views while preserving the artefact in full elsewhere.

Redaction does **not** mean:

- deletion,
- erasure,
- masking of content in-place.

Redaction is a relocation and reclassification operation.

---

## 5. Reasons for Redaction

Common reasons an artefact may be placed into the RRR include:

- Wrong patient association
- Misfiled demographic information
- Incorrect referral or care coordination entry
- Entered in error
- Consent withdrawal
- Jurisdictional or policy constraints

Reasons are recorded explicitly and separately from the artefact itself.

---

## 6. Relationship to Patient Repositories

When an artefact is redacted:

1. The artefact is removed from the relevant patient repository’s routine view.
2. A tombstone or pointer remains in the original location.
3. The artefact is stored in the RRR with full context and metadata.

The patient repository remains clinically clean while retaining traceability.

---

## 7. Access and Authorisation

Access to the RRR is:

- Role-based
- Audited
- Intended for legitimate purposes such as governance, investigation, correction, or legal response

RRR access is expected and normal for authorised roles.

---

## 8. What the RRR Is Not

The RRR is not:

- A temporary holding area
- A review queue
- A punishment mechanism
- A hidden or secret store
- A patient-facing record

---

## 9. Lifecycle Overview

- Artefact created in a patient repository
- Determination made that artefact should not appear in routine view
- Redaction action performed
- Artefact placed into RRR
- Tombstone retained in original context
- Artefact remains retained indefinitely

---

## 10. Future Considerations

- Retention classes and policies
- Cross-referencing with corrected or re-associated artefacts
- Reporting and metrics on redaction activity
- External regulatory access models

---

## 11. Summary

The Redaction Retention Repository is a foundational component of VPR that ensures integrity, transparency, and long-term trust in patient records by separating **routine use** from **permanent retention**, without loss of information.
