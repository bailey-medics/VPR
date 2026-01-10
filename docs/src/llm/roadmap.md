# VPR Development Roadmap

## Overview

**Purpose:** This roadmap outlines the planned development work for the Virtual Patient Record (VPR) system—a Git-backed electronic health record platform that provides verifiable, cryptographically-signed audit trails for every change to patient records. VPR treats patient data with the same rigour as source code, ensuring complete transparency, accountability, and long-term preservation of medical information.

**Priority Levels:**

- **Phase 1 (Foundation):** Epics 1-3 - Core data integrity and compliance
- **Phase 2 (Clinical Integration):** Epics 2, 4-5 - Healthcare standards and workflows  
- **Phase 3 (Production Readiness):** Epics 6-8 - Security, performance, and operations
- **Phase 4 (Patient Engagement):** Epic 9 - Patient data portability and autonomy

---

## Epics and Tasks

## Epic 1. Core Storage, Integrity, and Templates

**Business Value:** Establishes the foundational data storage architecture ensuring every patient record change is tracked, auditable, and tamper-evident. This is critical for regulatory compliance, medico-legal requirements, and patient safety.

**What it means:** Like a bank vault for medical records—every change is logged with who, what, when, and cryptographic proof it hasn't been altered. Built on Git version control to provide a permanent, verifiable audit trail.

- [x] File-based patient record store with sharded layout and per-patient Git repos (clinical + demographics separation).
- [x] Clinical template seeding and validation at startup (required directory exists and is copied into new clinical repos).
- [x] Commit-signing made optional during development (signing disabled by default in dev environments).
- [x] Integrate cargo-audit into CI/CD: automatically check dependencies for known security vulnerabilities on every build.
- [x] Integrate cargo-deny into CI/CD: enforce dependency licensing policies, detect banned crates, and check for unsafe code usage; configure allowed unsafe patterns and document exceptions.
- [ ] Harden template validation: enforce symlink bans, size/depth caps, and allowed file types; fail fast with clear errors.
- [ ] Tighten traversal and allocation limits for sharded patient discovery; cap patient count per listing call.
- [ ] Implement a retry and back-off strategy for filesystem and Git operations; clearly document which errors should not be retried.
- [ ] Perform thorough validation of all input data (such as patient identifiers and namespaces) before allowing any changes to occur; add any necessary protective checks where they are currently missing.
- [ ] Add monitoring capabilities to detect and report template validation failures through system logs and performance metrics.
- [ ] Consider `git gc` on repo periodically.
- [ ] Add patient comments (? unsigned).
- [ ] No symlinks ever. Need to parse incoming and current repos.

## Epic 2. openEHR Alignment and Reference Model (RM) Semantics

**Business Value:** Ensures VPR speaks the same language as international healthcare IT standards, enabling interoperability with other systems and preventing vendor lock-in. Compliance with openEHR standards future-proofs clinical data for decades.

**What it means:** Like ensuring our medical records can be understood by any hospital system worldwide, not just our own. openEHR is an internationally recognised standard for structuring clinical data that ensures longevity and portability of patient information.

- [ ] Clarify Reference Model (RM) system version handling and namespace expectations; document RM compatibility matrix.
- [ ] Ensure `ehr_status` and clinical content align with openEHR references (external_ref linkage to demographics).
- [ ] Map clinical template contents to openEHR archetype expectations; document any divergences.
- [ ] Add validation/linters (where practical) to catch RM/archetype mismatches early.
- [ ] Define and support initial data types for the first VPR build: `ehr_status`, clinical letters, documents (PDF), and patient-to-clinic messaging (storage layout, validation, and linkage to demographics/clinical records).

## Epic 3. Logging and Auditability

**Business Value:** Creates a complete, searchable audit trail of all system activities for compliance (GDPR, HIPAA), security incident investigation, and performance troubleshooting. Essential for demonstrating duty of care and regulatory compliance.

**What it means:** Every action in the system is logged—who accessed what record, when, and what they did. Like CCTV for data access, but with appropriate privacy protections. Enables answering "who viewed this patient's record?" or "why did this operation fail?"

- [ ] Define a consistent logging schema across all binaries and services (fields, levels, correlation identifiers/request identifiers).
- [ ] Ensure sensitive data handling: redact PHI (Protected Health Information) in logs; document safe logging patterns.
- [ ] Standardise error reporting in logs (error categories, causes, user-facing vs. internal detail).
- [ ] Correlate requests across REST/gRPC and core operations using propagated request identifiers.
- [ ] Document logging configuration, sinks, and rotation/retention expectations.

## Epic 4. Demographics via FHIR (Fast Healthcare Interoperability Resources)

**Business Value:** Implements patient demographics (name, date of birth, address, contact details) using FHIR, the global standard for healthcare data exchange. Enables integration with NHS systems, GP practices, and third-party health apps.

**What it means:** Patient demographic data stored in a format that any modern healthcare system can understand and exchange. FHIR is to healthcare what HTML is to websites—a universal standard that enables systems to talk to each other.

- [x] Demographics repository separated from clinical data with FHIR-like `patient.json` per patient.
- [ ] Need to copy the functionality of the clinical.rs module.
- [ ] Validate demographics against a chosen FHIR profile (fields, required elements, formats, namespaces).
- [ ] Add pagination/limits and stronger validation for demographics listing/queries.
- [ ] Document demographics data contract and evolution strategy.

## Epic 5. Coordination / Patient Administration System (PAS)-like Functions

**Business Value:** Tracks patient journeys through the healthcare system—appointments, referrals, episodes of care, bed management. Connects administrative workflows to clinical records, improving care coordination and reducing administrative burden.

**What it means:** The scheduling and tracking system that ensures patients get seen by the right clinician at the right time. Links appointments to clinical records so when a patient arrives for their cardiology follow-up, the cardiologist can see relevant history immediately.

- [ ] Define coordination domain model (encounters/episodes/appointments/referrals) and identifiers.
- [ ] Implement Care Coordination Repository: sharded storage under `patient_data/coordination/<s1>/<s2>/<uuid>/` with Git-backed versioning, aligned with clinical and demographics patterns.
- [ ] Link coordination artefacts to clinical and demographics records (stable references across repos).
- [ ] Authorisation model for coordination actions; align with patient-level access rules.
- [ ] Coordination data formats: define JSON/YAML schemas for encounters, appointments, episodes, and referrals; validate structure and required fields.
- [ ] Migration plan: create coordination shard subdirectories at startup; document coordination template or seed structure if required.

## Epic 6. Operational Hardening (Observability, Resilience, Backup, Performance, Security)

**Business Value:** Transforms VPR from a prototype into production-ready infrastructure that can be trusted with patient lives. Defines disaster recovery, performance targets, security controls, and monitoring needed for 24/7 clinical operations.

**What it means:** Making the system reliable enough to bet patient safety on. Includes: backup systems that can restore data if hardware fails, performance fast enough for busy clinics, security to prevent breaches, and monitoring to detect problems before they impact care.

**Risk:** Without this epic, VPR cannot be deployed in clinical settings. Regulatory approval and information governance sign-off depend on these controls.

- [ ] Standard tracing fields and request identifiers across services; ensure propagation through core calls.
- [ ] Error taxonomy and logging guidance (levels, redaction rules, PHI handling in logs).
- [ ] Metrics: key counters/histograms (latency, errors, template validation failures) and health signals.
- [ ] Define backup strategy for patient repos and templates (frequency, tooling, storage location).
- [ ] Restore drills: scripted restore into clean environment; verify integrity and signatures where applicable.
- [ ] Document Recovery Point Objective (RPO—maximum acceptable data loss) and Recovery Time Objective (RTO—maximum acceptable downtime) targets once agreed. **Question:** What RPO/RTO targets should we commit to for patient data and templates?
- [ ] Benchmark patient create/list under load; profile filesystem and Git hotspots.
- [ ] Set target latency/throughput SLOs (Service Level Objectives—performance promises) and document measurement method. **Question:** What latency/availability SLOs should we target for create/list/health?
- [ ] Optimise sharding/path operations if bottlenecks found; cache where safe.
- [ ] Define PHI (Protected Health Information—patient identifiable data) handling expectations and redaction rules across logs/metrics/storage. **Question:** What PHI redaction rules and boundaries are required across logs/metrics/storage?
- [ ] Decide storage encryption posture (at-rest options, filesystem or repository-level) and in-transit defaults; document key management for certificates and API keys. **Question:** Which encryption approach (filesystem/repository/KMS—Key Management Service) and key management/rotation policy should we adopt?
- [ ] **Finalise commit-signing policy for production:** Decide whether signing is mandatory for all records, or selectively applied (e.g., optional for patient-contributed comments). Document enforcement mechanism, certificate management, and consequences of unsigned commits in production. **Decision Required:** Must be resolved before production deployment.
- [ ] **Implement signature verification on read paths:** Add capability to verify Git commit signatures when reading patient data; make behaviour configurable (skip verification, warn on invalid/missing signatures, or fail hard and refuse to serve data). Coordinate with commit-signing policy decision.
- [ ] Integrate Miri into CI/CD: run Miri (Rust's interpreter) on test suite to detect undefined behaviour, use-after-free, and other subtle memory safety issues.
- [ ] Integrate sanitizers into CI/CD: configure AddressSanitizer, ThreadSanitizer, and MemorySanitizer for runtime detection of memory errors, data races, and uninitialized memory usage; run on comprehensive test suite.
- [ ] Resolve all cargo-audit warnings: address unmaintained dependencies (e.g., proc-macro-error from utoipa), either by updating to maintained alternatives, working with upstream maintainers, or documenting accepted risks in audit.toml with justification.
- [ ] GH dependabot
- [ ] Unit test src/main.rs

## Epic 7. API Transport and Auth Layer

**Business Value:** Provides secure, standardized interfaces for applications to access VPR data. Implements authentication to ensure only authorized systems/users can access patient records. Supports both modern web applications (REST) and high-performance inter-service communication (gRPC).

**What it means:** The secure doorway into the system. Like having both a traditional door lock (API keys) and biometric scanner (mTLS certificates) options. Ensures only authorised clinical applications can access patient data, and all access is authenticated and logged.

- [x] Dual API transports: gRPC (tonic) and REST (axum/utoipa) with shared protobuf types; health endpoints on both; optional gRPC reflection wiring present.
- [x] Basic auth guard on gRPC via API key interceptor.
- [ ] Turn off reflections in production
- [ ] REST auth parity with gRPC: enforce API key model (per decision: "yes"), document required header, error model, and config flags. **Question:** Is there a target timeline or environment scope for adding mTLS (mutual Transport Layer Security—certificate-based authentication)/alternative auth alongside API keys?
- [ ] Optional mTLS design (if needed later): propose cert layout, trust store management, and dual-mode support. **Question:** Should we plan mTLS now or later, and what trust distribution mechanism is preferred?
- [ ] Explicit error models for REST/gRPC (structured errors, consistent status mapping).
- [ ] Pagination and limits for listing APIs; input validation coverage for all request fields.
- [ ] Secrets handling: define how API keys are stored/rotated (env vs file vs secrets manager) and redaction in logs.
- [ ] Versioning strategy (semantic versioning) and changelog process.
- [ ] Upgrade/migration notes, including template or data layout changes.

## Epic 8. API Projection and Documentation Layer

**Business Value:** Optimises data retrieval performance for front-end applications and provides comprehensive API documentation for third-party integrators. Enables faster screen loads in clinical applications and reduces integration friction for partners.

**What it means:** Pre-computed views of data (like database indexes) that make common queries lightning-fast, plus clear documentation so developers building clinical apps know exactly how to use VPR's APIs. Like the difference between searching every file on your computer vs. using Spotlight.

- [ ] Add projections and caching layer for API reads: define projection formats, cache keys/TTL, and consistency rules with underlying Git-backed stores.
- [ ] Projections for speed optimisation
- [ ] Update OpenAPI/Proto docs and examples to match behaviours.
- [ ] Artefact builds: binaries and container images; publish instructions and SBOMs (Software Bill of Materials—dependency lists for security audits) if required.
- [ ] Pagination and limits for listing APIs; input validation coverage for all request fields. (Keep requirements in sync with projections.)
- [ ] Secrets handling: define how API keys are stored/rotated (env vs file vs secrets manager) and redaction in logs. (Coordinate with Epic 7 for auth alignment.)

## Epic 9. Patient Data Portability (Download and Upload)

**Business Value:** Empowers patients with true ownership of their medical records through standardised export and import capabilities. Supports patient autonomy, enables seamless transitions between healthcare providers, and fulfils GDPR/data portability requirements. Critical for patient engagement and regulatory compliance.

**What it means:** Patients can download their complete record as a portable archive (like exporting photos from iCloud) and upload records from previous providers. This enables patients to move between healthcare systems without losing their history, share records with specialists, or maintain personal backups. Essential for patient rights and data sovereignty.

**Risk:** Upload functionality must be heavily secured to prevent malicious file injection, repository corruption, or system compromise.

- [ ] Design patient download format: define archive structure (ZIP/TAR), include Git history vs. current snapshot, metadata about export (timestamp, version, scope).
- [ ] Implement patient download API: authentication, rate limiting, scope selection (full record vs. date range vs. specific data types), progress tracking for large exports.
- [ ] Download audit trail: log all patient download requests with timestamp, scope, and format for regulatory compliance.
- [ ] Design patient upload format specification: define accepted formats, version compatibility requirements, size limits, and structural requirements.
- [ ] Implement robust upload validation pipeline: file type whitelisting, size limits, depth/nesting caps, mandatory virus/malware scanning integration.
- [ ] **Critical: Symlink and dangerous content detection:** Scan uploaded archives for symlinks, executable code, shell scripts, and path traversal attempts; reject uploads containing forbidden content with clear error messages.
- [ ] Upload sanitisation: strip metadata, normalise file names, validate Git repository structure, verify commit integrity if Git history included.
- [ ] Implement upload reconciliation logic: handle conflicts between existing records and uploaded data, define merge strategies, preserve existing audit trail.
- [ ] Upload preview/dry-run capability: allow patients to see what will be imported before final commit; show conflicts, validation warnings, and changes.
- [ ] Patient upload API: authentication, chunked upload support for large files, progress tracking, rollback capability if validation fails.
- [ ] Upload audit trail: log all upload attempts (successful and failed), content validation results, and data integration outcomes.
- [ ] Define patient authentication/consent requirements for upload: ensure patient identity verification and explicit consent before allowing record modification.
- [ ] Document data portability formats and migration guides for patients transitioning between providers.
- [ ] **Question:** Should patients be able to upload records signed by other systems, or only unsigned personal records? How do we handle trust boundaries for external signatures?

---

## Open Questions (Requiring Leadership Decision)

These questions require input from clinical leadership, information governance, or technical governance boards:

- Runtime LLM (Large Language Model) features: not planned for foreseeable future (LLM remains contributor-assistance only).

---

## Glossary

**API (Application Programming Interface):** The methods by which software systems communicate with each other.

**CI (Continuous Integration):** Automated testing and building of software on each code change.

**FHIR (Fast Healthcare Interoperability Resources):** International standard for exchanging healthcare information electronically.

**Git:** Version control system that tracks all changes to files with complete history and audit trail.

**gRPC:** High-performance communication protocol for services to talk to each other (technical implementation detail).

**HIPAA:** US healthcare privacy law (Health Insurance Portability and Accountability Act).

**KMS (Key Management Service):** Secure system for storing and managing encryption keys.

**mTLS (mutual Transport Layer Security):** Certificate-based authentication where both parties prove their identity.

**openEHR:** International standard for modelling and storing clinical data in a vendor-neutral way.

**PAS (Patient Administration System):** Software managing appointments, admissions, referrals, and patient flow.

**PDF (Portable Document Format):** Standard document file format.

**PHI (Protected Health Information):** Any patient-identifiable health information that must be kept confidential.

**REST:** Common API style for web applications (Representational State Transfer).

**RM (Reference Model):** In openEHR, the foundational data structures for clinical information.

**RPO (Recovery Point Objective):** Maximum acceptable amount of data loss measured in time (e.g., "can lose up to 1 hour of data").

**RTO (Recovery Time Objective):** Maximum acceptable downtime (e.g., "must restore service within 4 hours").

**SBOM (Software Bill of Materials):** List of all software components and dependencies (for security auditing).

**SLO (Service Level Objective):** Performance target the system commits to achieving (e.g., "95% of requests complete within 200ms").

**TTL (Time To Live):** How long cached data remains valid before refresh.

---

*Document maintained by: Technical Team*  
*Last updated: January 2026*  
*Next review: Quarterly or upon major architectural decisions*
