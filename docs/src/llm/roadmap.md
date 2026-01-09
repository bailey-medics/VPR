# Epics (numbered) and Tasks

## Epic 1. Core Storage, Integrity, and Templates

- [x] File-based patient record store with sharded layout and per-patient Git repos (clinical + demographics separation).
- [x] Clinical template seeding and validation at startup (required directory exists and is copied into new clinical repos).
- [ ] Commit-signing policy: decide mandatory vs optional (some patients/family records may skip signing); document environment defaults (Continuous Integration (CI) vs development). **Question:** Should signing be mandatory everywhere, or optional in development/specified patient contexts? How should read paths verify or surface unsigned content?
- [ ] Verification on read paths: define whether reads verify signatures and how to surface failures. **Question:** Should read operations fail hard on signature issues, warn, or gate behind a flag?
- [ ] Harden template validation: enforce symlink bans, size/depth caps, and allowed file types; fail fast with clear errors.
- [ ] Tighten traversal and allocation limits for sharded patient discovery; cap patient count per listing call.
- [ ] Retry/back-off policy for filesystem/Git operations; document non-retriable errors.
- [ ] Exhaustive validation of inputs (identifiers (IDs), namespaces) before side effects; add missing guards where needed.
- [ ] Monitoring hooks: surface template validation failures in logs/metrics.

## Epic 2. openEHR Alignment and Reference Model (RM) Semantics

- [ ] Clarify Reference Model (RM) system version handling and namespace expectations; document RM compatibility matrix.
- [ ] Ensure `ehr_status` and clinical content align with openEHR references (external_ref linkage to demographics).
- [ ] Map clinical template contents to openEHR archetype expectations; document any divergences.
- [ ] Add validation/linters (where practical) to catch RM/archetype mismatches early.
- [ ] Define and support initial data types for the first VPR build: `ehr_status`, clinical letters, documents (Portable Document Format (PDF)), and patient-to-clinic messaging (storage layout, validation, and linkage to demographics/clinical records).

## Epic 3. Logging and Auditability

- [ ] Define a consistent logging schema across all binaries and services (fields, levels, correlation identifiers/request identifiers).
- [ ] Ensure sensitive data handling: redact Protected Health Information (PHI) in logs; document safe logging patterns.
- [ ] Standardise error reporting in logs (error categories, causes, user-facing vs internal detail).
- [ ] Correlate requests across REST/gRPC and core operations using propagated request identifiers.
- [ ] Document logging configuration, sinks, and rotation/retention expectations.

## Epic 4. Demographics via Fast Healthcare Interoperability Resources (FHIR)

- [x] Demographics repository separated from clinical data with FHIR-like `patient.json` per patient.
- [ ] Need to copy the functionality of the clinical.rs module.
- [ ] Validate demographics against a chosen FHIR profile (fields, required elements, formats, namespaces).
- [ ] Add pagination/limits and stronger validation for demographics listing/queries.
- [ ] Document demographics data contract and evolution strategy.

## Epic 5. Coordination / Patient Administration System (PAS)-like Functions

- [ ] Define coordination domain model (encounters/episodes/appointments/referrals) and identifiers.
- [ ] Link coordination artifacts to clinical and demographics records (stable references across repos).
- [ ] Authorisation model for coordination actions; align with patient-level access rules.
- [ ] Persistence layout and sharding approach for coordination data; migration plan if new directories/repos are introduced.

## Epic 6. Operational Hardening (Observability, Resilience, Backup, Performance, Security)

- [ ] Standard tracing fields and request identifiers across services; ensure propagation through core calls.
- [ ] Error taxonomy and logging guidance (levels, redaction rules, PHI handling in logs).
- [ ] Metrics: key counters/histograms (latency, errors, template validation failures) and health signals.
- [ ] Define backup strategy for patient repos and templates (frequency, tooling, storage location).
- [ ] Restore drills: scripted restore into clean environment; verify integrity and signatures where applicable.
- [ ] Document Recovery Point Objective (RPO) and Recovery Time Objective (RTO) targets once agreed. **Question:** What RPO/RTO targets should we commit to for patient data and templates?
- [ ] Benchmark patient create/list under load; profile filesystem and Git hotspots.
- [ ] Set target latency/throughput Service Level Objectives (SLOs) (first define SLO expectations) and document measurement method. **Question:** What latency/availability SLOs should we target for create/list/health?
- [ ] Optimise sharding/path operations if bottlenecks found; cache where safe.
- [ ] Define Protected Health Information (PHI) handling expectations and redaction rules across logs/metrics/storage. **Question:** What PHI redaction rules and boundaries are required across logs/metrics/storage?
- [ ] Decide storage encryption posture (at-rest options, filesystem or repository-level) and in-transit defaults; document key management for certificates and API keys. **Question:** Which encryption approach (filesystem/repository/Key Management Service (KMS)) and key management/rotation policy should we adopt?

## Epic 7. API Transport and Auth Layer

- [x] Dual API transports: gRPC (tonic) and REST (axum/utoipa) with shared protobuf types; health endpoints on both; optional gRPC reflection wiring present.
- [x] Basic auth guard on gRPC via API key interceptor.
- [ ] REST auth parity with gRPC: enforce API key model (per decision: “yes”), document required header, error model, and config flags. **Question:** Is there a target timeline or environment scope for adding mutual Transport Layer Security (mTLS)/alternative auth alongside API keys?
- [ ] Optional mTLS design (if needed later): propose cert layout, trust store management, and dual-mode support. **Question:** Should we plan mTLS now or later, and what trust distribution mechanism is preferred?
- [ ] Explicit error models for REST/gRPC (structured errors, consistent status mapping).
- [ ] Pagination and limits for listing APIs; input validation coverage for all request fields.
- [ ] Secrets handling: define how API keys are stored/rotated (env vs file vs secrets manager) and redaction in logs.
- [ ] Versioning strategy (semantic versioning) and changelog process.
- [ ] Upgrade/migration notes, including template or data layout changes.

## Epic 8. API Projection and Documentation Layer

- [ ] Add projections and caching layer for API reads: define projection formats, cache keys/TTL, and consistency rules with underlying Git-backed stores.
- [ ] Update OpenAPI/Proto docs and examples to match behaviours.
- [ ] Artefact builds: binaries and container images; publish instructions and SBOMs if required.
- [ ] Pagination and limits for listing APIs; input validation coverage for all request fields. (Keep requirements in sync with projections.)
- [ ] Secrets handling: define how API keys are stored/rotated (env vs file vs secrets manager) and redaction in logs. (Coordinate with Epic 8 for auth alignment.)

## Open Questions (tracked, some answered)

- Runtime Large Language Model (LLM) features: not planned for a long time (LLM remains contributor-assistance only).
