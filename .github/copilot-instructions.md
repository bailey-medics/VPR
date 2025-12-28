<!--
Guidance for AI coding agents working on the VPR repository.
Focus: be pragmatic, reference concrete files, and keep changes minimal and well-tested.
-->
# VPR — AI contributor notes

These notes are for automated coding agents and should be short, concrete, and codebase-specific.

Overview
- Purpose: VPR is a file-based patient record system with Git-like versioning, built as a Rust Cargo workspace. It provides dual gRPC and REST APIs for health checks and patient creation. The system stores patient data as JSON/YAML files in a sharded directory structure under `patient_data/`, with each patient having their own Git repository for version control.
- Key crates:
  - `crates/core` (vpr-core) — **PURE DATA OPERATIONS ONLY**: File/folder management, patient data CRUD, Git versioning with X.509 commit signing. **NO API concerns** (authentication, HTTP/gRPC servers, service interfaces).
  - `crates/api-shared` — Shared utilities and definitions for both APIs: Protobuf types, HealthService, authentication utilities.
  - `crates/api-grpc` — gRPC-specific implementation: VprService, authentication interceptors, tonic integration.
  - `crates/api-rest` — REST-specific implementation: HTTP endpoints, OpenAPI/Swagger, axum integration.
  - `crates/certificates` (vpr-certificates) — Digital certificate generation utilities: X.509 certificate creation for user authentication and commit signing.
  - `crates/cli` (vpr-cli) — Command-line interface: CLI tools for patient record management and certificate generation.
- Main binary: `vpr-run` (defined in root `Cargo.toml`), runs both gRPC (port 50051) and REST (port 3000) servers concurrently using tokio::join.

Important files to reference
- `src/main.rs` — Main binary that performs startup validation (checks for patient_data, ehr-template directories; creates clinical/demographics subdirs), creates runtime constants, and starts both gRPC (port 50051) and REST (port 3000) servers concurrently using tokio::join.
- `crates/core/src/lib.rs` — **PURE DATA OPERATIONS**: Services for file/folder operations (sharded storage, directory traversal, Git repos per patient). **NO API CODE**.
- `crates/core/src/clinical.rs` — ClinicalService: Initializes patients with EHR template copy, creates Git repo, signs commits with X.509.
- `crates/core/src/demographics.rs` — DemographicsService: Updates patient demographics JSON, lists patients via directory traversal.
- `crates/api-grpc/src/service.rs` — gRPC service implementation (VprService) with authentication, using core services.
- `crates/api-shared/vpr.proto` — Canonical protobuf definitions for VPR service (note: national_id field present but unused in current impl).
- `crates/api-shared/src/health.rs` — Shared HealthService used by both gRPC and REST APIs.
- `Justfile` — Developer commands: `just start-dev` (Docker dev), `just docs` (mdBook site), `just pre-commit`.
- `compose.dev.yml` — Development Docker setup with cargo-watch live reload and healthcheck (`grpcurl -plaintext localhost:50051 list && curl -f http://localhost:3000/health`).
- `scripts/check-all.sh` — Quality checks: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo check`, `cargo test`.
- `docs/src/overview.md` — Detailed project overview and architecture.

Build and test workflows (concrete)
- Local quick compile: `cargo build -p api-grpc` (or `cargo run` for full binary).
- Full workspace checks: `./scripts/check-all.sh` (runs fmt, clippy, check, test).
- Docker dev runtime: `just start-dev` or `docker compose -f compose.dev.yml up --build`.
- Healthcheck: `grpcurl -plaintext localhost:50051 list` (gRPC) and `curl http://localhost:3000/health` (REST).
- Documentation: `just docs` serves mdBook site with integrated rustdoc.

Conventions and patterns to follow
- Protobufs: Canonical proto in `crates/api-shared/vpr.proto`; generated Rust in `api_shared::pb` via build script.
- Service wiring: `crates/api-grpc` implements `VprService` using core services; main.rs uses `api_grpc::VprService`.
- Patient storage: Sharded under `PATIENT_DATA_DIR` (env var, defaults to `patient_data`): 
  - Clinical: `clinical/<s1>/<s2>/<32hex-uuid>/` (ehr_status.yaml, copied ehr-template files, Git repo)
  - Demographics: `demographics/<s1>/<s2>/<32hex-uuid>/patient.json` (FHIR-like JSON, Git repo)
  where s1/s2 are first 4 hex chars of UUID.
- APIs: Dual gRPC/REST with identical functionality; REST uses axum, utoipa for OpenAPI.
- Logging: `tracing` with `RUST_LOG` env var (e.g., `vpr=debug`).
- Error handling: tonic `Status` for gRPC, axum `StatusCode` for REST; internal errors logged with `tracing::error!`.
- File I/O: Direct `std::fs` operations with `serde_json`/`serde_yaml` for patient data; no database layer.
- Git versioning: Each patient directory is a Git repo; commits signed with X.509 certificates from author.signature.
- EHR template: `ehr-template/` directory copied to new patient clinical dirs; validated at startup.
- **Architecture boundaries**: 
  - `core`: ONLY file/folder/git operations (ClinicalService, DemographicsService, data persistence)
  - `api-shared`: Shared API utilities (HealthService, auth, protobuf types)
  - `api-grpc`: gRPC-specific concerns (service implementation, interceptors)
  - `api-rest`: REST-specific concerns (HTTP endpoints, JSON handling)
  - `main.rs`: Startup validation (patient_data, ehr-template dirs), runtime constants, service orchestration

Change policy and safety
- Prefer minimal, well-scoped PRs updating single crates or modules.
- Run `./scripts/check-all.sh` before proposing changes; fix clippy warnings.
- When changing protos: Update `crates/api-shared/vpr.proto`, regenerate with `cargo build`.
- Patient data paths: Hardcoded sharding logic in `core`; avoid changing without testing directory traversal in `list_patients`.
- Environment config: Use env vars like `VPR_ADDR`, `PATIENT_DATA_DIR`; defaults in code.
- Proto fields: Some fields (e.g., national_id) present but unused in current implementation.
- **Architecture boundaries**: 
  - Never add API concerns (auth, HTTP/gRPC) to `crates/core` - keep it pure data operations
  - Shared API functionality goes in `crates/api-shared`, not individual API crates
  - API-specific code stays in respective `api-grpc` or `api-rest` crates

Examples (copyable snippets)
- Start dev servers: `just start-dev b` (builds and runs Docker containers).
- Health check: `grpcurl -plaintext localhost:50051 vpr.v1.VPR/Health` or `curl http://localhost:3000/health`.
- Create patient: `grpcurl -plaintext -d '{"first_name":"John","last_name":"Doe"}' localhost:50051 vpr.v1.VPR/CreatePatient`.
- List patients: `grpcurl -plaintext localhost:50051 vpr.v1.VPR/ListPatients` or `curl http://localhost:3000/patients`.

Edge cases for automated edits
- Do not change workspace members in root `Cargo.toml` without verifying all crates build.
- Avoid altering patient directory sharding in `core/src/lib.rs` — `list_patients` relies on exact structure.
- Main.rs runs both servers; changes must maintain concurrency (tokio::join).
- Docker mounts `./patient_data` for persistence; test with actual file creation/deletion.
- EHR template validation: `ehr-template/` must exist and contain files; clinical init copies it recursively.

If unsure, ask for clarification and provide a short plan: files to change, tests to add, and commands you will run to validate.

---
If you'd like I can expand any section (e.g., CI, proto build details, or example PR checklist).

