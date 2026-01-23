# VPR Development Roadmap

## Overview

**Purpose:**

This roadmap outlines the planned development work for the Versioned Patient Repository (VPR) – a Git-backed clinical record system designed to preserve verifiable clinical truth, authorship, and history over decades. VPR treats patient records as durable, inspectable artefacts with explicit provenance, rather than mutable database rows.

**Guiding principles:**

- Patients first and human readable
- Clinical truth is append-only and auditable

---

## Phase Grouping

- **Phase 1 – Foundations of Truth:** Epics 1–3
- **Phase 2 – Semantics and Meaning:** Epics 4–6
- **Phase 3 – Operational Reality:** Epics 7–9
- **Phase 4 – Access, Projections, and Record upload:** Epics 10–15

---

## Epic 1. Core Storage, Integrity, and Templates

**Business Value:**

Establishes the foundational storage and integrity model for VPR. Every patient record change is durable, inspectable, and tamper-evident.

- [x] File-based patient record store with sharded layout and per-patient Git repositories (clinical + demographics separation)
- [x] Clinical template seeding and validation at startup
- [x] Commit signing optional in development environments
- [x] Integrate cargo-audit into CI/CD
- [x] Integrate cargo-deny into CI/CD
- [ ] Tighten traversal and allocation limits for patient discovery
- [ ] Implement retry and back-off strategy for filesystem and Git operations
- [ ] Validate all user-supplied identifiers and namespaces before side effects
- [ ] Add monitoring for template validation failures
- [ ] Conservative `git gc` strategy for per-patient repos
- [ ] Enforce “no symlinks ever” policy across templates, imports, and repos

---

## Epic 2. openEHR Alignment and Reference Model Semantics

**Business Value:**  
Ensures long-term interoperability while preventing openEHR wire models from contaminating internal domain logic.

- [ ] Define supported openEHR RM versions and validation strategy
- [ ] Specify namespace formation and validation rules
- [ ] Publish RM/namespace compatibility matrix per deployment
- [ ] Validate `ehr_status` linkage to demographics (`external_ref`)
- [ ] Map clinical templates to openEHR archetype expectations
- [ ] Add RM/archetype validation or linting where practical
- [ ] Define supported artefact types:
  - [ ] `ehr_status.yaml`
  - [ ] Clinical letters (Markdown with YAML front matter)
  - [ ] Documents (PDF with sidecar metadata)
  - [ ] Structured messaging threads (YAML/JSON)
- [ ] Implement large-file-storage for binary artefacts (PDFs, images, scans) outside Git to preserve repository performance
- [ ] Support patient-contributed artefacts and annotations
- [ ] Explicitly document boundary between wire models and internal domain models

---

## Epic 3. Demographics via FHIR

**Business Value:**  
Separates patient identity from clinical truth while enabling interoperability.

- [x] Separate demographics repository (FHIR-like `patient.json`)
- [ ] Implement demographics service parity with clinical service
- [ ] Validate demographics against selected FHIR profile
- [ ] Pagination and limits for demographics listing and queries
- [ ] Document demographics data contract and evolution strategy

---

## Epic 4. Clinical Record Lifecycle and Semantic States

**Business Value:**  
Removes ambiguity about what a clinical record _means_ over time.

- [ ] Define lifecycle states (created, amended, corrected, superseded, closed)
- [ ] Define metadata conventions for lifecycle state
- [ ] Distinguish “wrong at the time” vs “correct then, obsolete now”
- [ ] Define closure and reopening semantics
- [ ] Document how consumers should interpret lifecycle state
- [ ] Explicitly document what VPR does not infer automatically

---

## Epic 5. Temporal Semantics and Clinical Time

**Business Value:**  
Ensures timestamps are clinically and legally interpretable.

- [ ] Define event time vs documentation time vs commit time
- [ ] Support retrospective documentation
- [ ] Define correction and amendment timing semantics
- [ ] Handle clock skew and external system timestamps
- [ ] Document required and optional temporal fields per artefact type
- [ ] Ensure Git commit time is never misrepresented as clinical event time

---

## Epic 6. Logging, Auditability, and Provenance

**Business Value:**  
Supports investigation, compliance, and forensic reconstruction.

- [ ] Define structured logging schema
- [ ] Enforce PHI redaction rules in logs
- [ ] Standardise error taxonomy
- [ ] Correlate operations with request IDs and commit hashes
- [ ] Log validation, security, and auth failures
- [ ] Log operational signals (retries, maintenance tasks)
- [ ] Document log retention, sinks, and access controls

---

## Epic 7. Failure Modes and Recovery Semantics

**Business Value:**  
Ensures predictable behaviour on bad days.

- [ ] Enumerate supported failure modes (partial writes, corruption, tampering)
- [ ] Classify failures (fatal, recoverable, operator intervention)
- [ ] Define system behaviour per failure class
- [ ] Define which failures must always be surfaced to operators
- [ ] Document guarantees around non-silent failure

---

## Epic 8. Operational Hardening and Catastrophic Recovery

**Business Value:**  
Ensures patient data survives hardware failure, human error, and attack.

- [ ] Define write-through backup strategy for patient repos
- [ ] Physically and administratively separate backup storage
- [ ] Offline cold backups at defined intervals
- [ ] Restore drills into clean environments
- [ ] Verify integrity and signatures on restore
- [ ] Define and document RPO and RTO targets
- [ ] Implement recovery marker commits with provenance
- [ ] Guarantee no silent history rewriting during restore
- [ ] Define encryption-at-rest and key management posture
- [ ] Finalise commit-signing policy for production
- [ ] Implement configurable signature verification on read paths

---

## Epic 9. Governance, Authority, and Evolution Boundaries

**Business Value:**  
Prevents architectural drift and unresolvable disputes.

- [ ] Define authority for RM version acceptance and deprecation
- [ ] Define schema evolution and incompatibility handling
- [ ] Document which decisions live outside the codebase
- [ ] Define escalation paths for semantic disputes
- [ ] Explicitly separate technical enforcement from organisational policy

---

## Epic 10. Care Coordination and PAS-like Functions

**Business Value:**  
Supports operational workflows without polluting clinical truth.

- [ ] Define coordination domain model (encounters, referrals, appointments)
- [ ] Implement Care Coordination Repository with Git-backed storage
- [ ] Link coordination artefacts to clinical and demographics records
- [ ] Define authorisation rules for coordination actions
- [ ] Define YAML schemas for coordination artefacts
- [ ] Support UX state (read/unread, task completion)
- [ ] Explicitly document non-authoritative status vs clinical record

---

## Epic 11. API Transport, Auth, and Contracts

**Business Value:**  
Provides secure, well-defined access to VPR.

- [x] REST and gRPC transports with shared protobufs
- [x] API key authentication for gRPC
- [ ] Configuration options to enable/disable gRPC and/or REST APIs independently (allow both, either, or neither)
- [ ] Disable reflection in production
- [ ] REST authentication parity with gRPC
- [ ] Optional mTLS design (future)
- [ ] Structured error models for REST and gRPC
- [ ] Pagination and validation for all listing APIs
- [ ] Secrets storage and rotation strategy
- [ ] API versioning and upgrade documentation

---

## Epic 12. Read Models, Projections, and Performance

**Business Value:**  
Improves performance without betraying truth.

- [ ] Define projection formats and cache semantics
- [ ] Explicitly mark projections as non-authoritative
- [ ] Ensure projections are disposable and rebuildable
- [ ] Link projections back to commit hashes
- [ ] Benchmark read and write paths under load
- [ ] Document acceptable projection lag

---

## Epic 13. Patient Data Portability and Agency

**Business Value:**  
Supports patient autonomy and regulatory compliance.

- [ ] Define patient download formats (full history vs snapshot)
- [ ] Implement authenticated download APIs
- [ ] Log and audit all patient downloads
- [ ] Define accepted upload formats and version compatibility
- [ ] Implement robust upload validation and sanitisation
- [ ] Reject symlinks, executables, and path traversal
- [ ] Support upload dry-run and preview
- [ ] Define merge and reconciliation strategies
- [ ] Log and audit all upload attempts
- [ ] Define trust boundaries for externally signed records

---

## Epic 14. Education, Invariants, and Operational Literacy

**Business Value:**  
Reduces institutional memory risk and misuse.

- [ ] Operator runbooks (backup, restore, failure handling)
- [ ] Developer invariants (what must never be violated)
- [ ] “What VPR does not do” documentation
- [ ] Shared mental model for contributors and operators

---

## Epic 15. Core and Organisational Separation

**Business Value:**  
Keeps the patient-record core reusable as a standalone library for patient/self-hosted deployments while allowing organisational layers (security, APIs, projections, back-office) to evolve independently without contaminating core invariants.

- [ ] Define the boundary for a standalone core library crate (patient data model, filesystem/Git, validation) that excludes organisational concerns.
- [ ] Document dependency direction: core must not depend on organisational code; organisational layers may depend on core.
- [ ] Identify organisational-only modules (authentication/authorisation, API transport, projections/cache, observability/ops) to reside outside the core crate.
- [ ] Evaluate packaging and repository split options (single repo with crates vs separate repositories) and their impact on versioning and CI.
- [ ] Plan migration and testing strategy for the split (CI matrices, contract tests, release cadence, documentation updates).

---
