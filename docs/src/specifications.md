# VPR – Versioned Patient Repository

> **Note:** This document provides a high-level overview. For detailed technical specifications, see [LLM Specification](llm/spec.md).

## Purpose

- Store patient records in a version-controlled manner, using Git.
- Serve those records fast to clinicians, admins, or patients.
- Keep everything accurate, secure, and auditable.

## Technology Choices

- Rust for everything (fast, safe, compiled to a single binary).
- gRPC and REST APIs for system integration (fast, typed communication between systems).
- Git as the underlying truth for documents (every version saved, nothing silently overwritten).
- File-based storage with sharded directory structure for scalability.
- Future: database projections (Postgres) and caching (Redis) for performance optimisation (planned).

## Data Model

- Records are stored as YAML and Markdown files inside Git repositories, versioned automatically.
- Each patient has three separate Git repositories:
  - **Clinical repository**: openEHR-based clinical content (observations, diagnoses, clinical letters)
  - **Demographics repository**: FHIR-based patient demographics (name, date of birth, identifiers)
  - **Coordination repository** (Care Coordination Repository): care coordination data (encounters, appointments, episodes, referrals) – format to be determined, may adopt FHIR ideologies
- Patient data is sharded: `patient_data/{clinical,demographics,coordination}/<s1>/<s2>/<uuid>/` where s1/s2 are first 4 hex chars of UUID.
- Every new change makes a new Git commit, never overwriting the old one.
- Commits can be cryptographically signed (ECDSA P-256) for authorship verification.

## API

- Dual transport: gRPC (tonic) and REST (axum/utoipa).
- Create patient – initialise new patient with demographics and clinical template.
- List patients – retrieve patient list from sharded directory structure.
- Health endpoints – confirm service availability.
- API authentication via API keys (gRPC and REST when enabled).
- OpenAPI/Swagger documentation for REST endpoints.

## Security

- All communication uses encryption (TLS).
- API key authentication for gRPC; REST authentication configurable.
- Optional mTLS support planned.
- Data on disk can be encrypted if required.
- Commit signing with X.509 certificates for authorship verification.
- PHI redaction in logs and metrics.

## Corrections & Deletions

- Normal use is append-only (you don’t delete history).
- If wrong patient data is added:
  - Prefer redaction (mark as wrong but leave audit trail).
  - If legally required, remove with a special process (cryptographic erase or repo rewrite).

## Performance Approach

- Sharded directory structure to maintain predictable filesystem performance.
- Clinical template seeded from validated template directory at patient creation.
- Future: database projections and caching layer for API reads (planned).
- Git operations per-patient ensure isolation and manageable repository sizes.

## Reliability

- Every change tracked in Git with complete audit trail.
- Provenance (who did what and when) captured in Git commit metadata.
- Commit signatures provide cryptographic proof of authorship where configured.
- Defensive programming: validate inputs before side effects, fail fast on invalid config.

## Operations

- Runs as dual-service binary (`vpr-run`) or standalone gRPC/REST services.
- Configured by environment variables (patient data dir, clinical template dir, RM system version, namespace, API keys, bind addresses).
- CLI tool (`vpr-cli`) for administrative tasks.
- Docker development environment with live reload.
- Quality checks: `./scripts/check-all.sh` (fmt, clippy, check, test).

##  Cargo features

- A feature flag for code builds.

Features needed for a patient to view and edit their own records:

```bash
cargo build --features patient
```

Features needed for clinicians and admins to manage records in a multi-patient environment:

```bash
cargo build --features org
```

## Architecture Boundaries

- `crates/core` – Pure data operations: file/folder management, Git versioning, patient data CRUD. No API concerns.
- `crates/api-shared` – Shared utilities: Protobuf types, HealthService, authentication.
- `crates/api-grpc` – gRPC-specific implementation: VprService, interceptors.
- `crates/api-rest` – REST-specific implementation: HTTP endpoints, OpenAPI.
- `crates/certificates` – X.509 certificate generation for authentication and commit signing.
- `crates/cli` – Command-line interface for administrative operations.

## Wrong patient

- Redact
- Stub
  - Preserve cryptographic proof of what was removed
  - Hashed Message Authentication Code (mathematical fingerprint of the original data)
- Quarantine vault
  - Quarantine bytes

> tombstone locally, escrow the content in a restricted space, and leave a non-revealing hash pointer for audit.
