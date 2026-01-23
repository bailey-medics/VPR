# VPR Large File and Binary Storage

## Purpose

This document defines how large binary files (for example PDFs, imaging, scans, waveforms, audio, and video) are stored, referenced, versioned, and governed within the Versioned Patient Repository (VPR).

The aim is to preserve clinical meaning, auditability, and long-term safety while avoiding architectural coupling between clinical records and bulk binary storage.

---

## Core Principles

- Clinical meaning and bulk data are deliberately separated
- Binary data is immutable once stored
- Identity is based on content, not file paths or mutable locations
- Versioning and auditability are explicit and permanent
- Storage lifecycles for binaries are independent of clinical records

---

## What Counts as a Large File

Large files include, but are not limited to:

- Portable Document Format (PDF) documents
- Medical imaging (for example radiology images)
- Scanned paper documents
- Audio or video recordings
- Physiological waveforms or monitoring exports

These files are treated as **binary evidence objects**, not primary clinical records.

---

## Architectural Separation

VPR separates responsibilities into distinct layers:

- **Demographics Repository (DR)** – patient identity and demographics
- **Clinical Repository (CR)** – clinical facts and artefacts
- **Care Coordination Repository (CCR)** – workflow, messaging, and coordination
- **Redaction Retention Repository (RRR)** – redacted or withdrawn material
- **Binary Object Store (BOS)** – storage of large binary data

The Binary Object Store is a sibling system to VPR repositories, not part of them.

---

## Binary Object Store (BOS)

### Role

The Binary Object Store is responsible for:

- Efficient storage of large binary files
- Encryption, replication, and durability
- Enforcement of retention and deletion policies
- Serving binary data when authorised

The BOS does **not** in itself encode clinical meaning.

---

### Required Properties

The Binary Object Store MUST support:

- Content-addressed storage (hash-based identity)
- Immutability after write
- Retrieval by content identifier
- Independent lifecycle management
- Secure deletion and redaction mechanisms

The specific implementation (cloud object storage, on-premise storage, or other) is deliberately abstracted.

---

## Binary Identity

Each binary object is identified by:

- A cryptographic content hash
- A declared checksum algorithm

The content hash is the canonical identifier.

If file contents change for any reason, a **new binary object** MUST be created.

---

## Binary References in VPR

### Purpose of a Binary Reference

VPR never stores large binaries directly.

Instead, it stores **Binary References** which:

- Assert the existence of a binary
- Describe its clinical role
- Bind it immutably to a point in time

Binary References are small, human-readable, and versionable.

---

### Typical Reference Metadata

A Binary Reference typically records:

- Binary identifier (content hash)
- Media type
- Size in bytes
- Creation timestamp
- Author or source
- Clinical context or purpose
- Storage backend (abstract identifier, not a URL)
- Checksum algorithm

The reference is immutable once committed.

---

## Placement Rules

Binary References are stored **where the clinical meaning lives**:

- Clinical documents (letters, reports, results) → Clinical Repository
- Workflow or administrative artefacts → Care Coordination Repository
- Withdrawn or redacted references → Redaction Retention Repository

Origin of the file (patient, clinician, external organisation) does not determine placement.

Clinical meaning does.

---

## External and Patient-Provided Documents

Patient-provided or externally received documents follow a triage process:

1. The binary is stored in the Binary Object Store
2. An initial reference may exist in Care Coordination
3. A clinician reviews the content
4. If clinical facts are asserted, a clinical artefact referencing the binary is created in the Clinical Repository

This mirrors real-world clinical workflows.

---

## Versioning Behaviour

- Binary content is immutable
- References are append-only
- Superseding a document creates a new reference
- Historical references remain valid indefinitely

No reference is silently replaced or overwritten.

---

## Redaction and Deletion

VPR does not support silent deletion of clinical artefacts or their history.

Redaction is handled explicitly and involves coordinated actions across systems:

- A binary reference in the Clinical or Care Coordination Repository is marked as redacted or withdrawn, leaving an immutable tombstone
- The associated artefact and governance metadata are moved to the Redaction Retention Repository
- The Binary Object Store may revoke access to the binary or cryptographically destroy its contents, but only as the result of a recorded redaction event

VPR always retains evidence that the binary existed, even when its content is no longer accessible.

Deletion is treated as a recorded event, never as an absence.

---

## Why Git Large File Storage Is Not Used

Git Large File Storage is not suitable because:

- It identifies files by path and repository state, not content identity
- It does not enforce immutability of stored binaries
- It couples clinical records to developer tooling
- It cannot support independent retention and redaction lifecycles
- It requires specialist tooling to interpret references

VPR uses Git concepts as inspiration, not as a runtime dependency.

---

## Summary

- Large binaries are stored outside VPR
- VPR stores immutable, content-addressed references
- Clinical meaning determines placement
- Auditability and long-term safety take precedence over convenience

This design ensures VPR remains inspectable, defensible, and clinically trustworthy over decades.
