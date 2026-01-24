<!--
Guidance for AI coding agents working on the VPR repository.
Focus: be pragmatic, reference concrete files, and keep changes minimal and well-tested.
-->

# VPR — AI contributor notes

These notes are for automated coding agents and should be short, concrete, and codebase-specific.

Specifications live in [spec.md](spec.md); roadmap is tracked in [roadmap.md](roadmap.md). Keep this document consistent with those sources.

Overview

- Purpose: VPR is a file-based patient record system with Git-like versioning, built as a Rust Cargo workspace. It provides dual gRPC and REST APIs for health checks and patient creation. The system stores patient data as JSON/YAML files in a sharded directory structure under `patient_data/`, with each patient having their own Git repositories (clinical, demographics, and coordination) for version control.
- Key crates:
  - `crates/core` (vpr-core) — **PURE DATA OPERATIONS ONLY**: File/folder management, patient data CRUD, Git versioning with X.509 commit signing. **NO API concerns** (authentication, HTTP/gRPC servers, service interfaces).
  - `crates/api-shared` — Shared utilities and definitions for both APIs: Protobuf types, HealthService, authentication utilities.
  - `crates/api-grpc` — gRPC-specific implementation: VprService, authentication interceptors, tonic integration.
  - `crates/api-rest` — REST-specific implementation: HTTP endpoints, OpenAPI/Swagger, axum integration.
  - `crates/certificates` (vpr-certificates) — Digital certificate generation utilities: X.509 certificate creation for user authentication and commit signing.
  - `crates/cli` (vpr-cli) — Command-line interface: CLI tools for patient record management and certificate generation.
- Main binary: `vpr-run` (defined in root `Cargo.toml`), runs both gRPC (port 50051) and REST (port 3000) servers concurrently using tokio::join.

Important files to reference

- `src/main.rs` — Main binary that performs startup validation (checks for patient_data, clinical-template directories; creates clinical/demographics/coordination subdirs), creates runtime constants, and starts both gRPC (port 50051) and REST (port 3000) servers concurrently using tokio::join.
- `crates/core/src/lib.rs` — **PURE DATA OPERATIONS**: Services for file/folder operations (sharded storage, directory traversal, Git repos per patient). **NO API CODE**.
- `crates/core/src/config.rs` — `CoreConfig` and helpers used to resolve/validate configuration **once at startup**.
- `crates/core/src/clinical.rs` — ClinicalService: Initialises patients with clinical template copy, creates Git repo, signs commits with X.509.
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
- Service wiring: `crates/api-grpc` implements `VprService` using core services; binaries construct it via `VprService::new(Arc<CoreConfig>)`.
- Patient storage: Sharded under the configured patient data directory (default: `patient_data`):
  - Clinical: `clinical/<s1>/<s2>/<32hex-uuid>/` (ehr_status.yaml, copied clinical-template files, Git repo)
  - Demographics: `demographics/<s1>/<s2>/<32hex-uuid>/patient.json` (FHIR-like JSON, Git repo)
  - Coordination: `coordination/<s1>/<s2>/<32hex-uuid>/` (Care Coordination Repository: encounters, appointments, episodes, referrals; Git repo)
    where s1/s2 are first 4 hex chars of UUID.
- APIs: Dual gRPC/REST with identical functionality; REST uses axum, utoipa for OpenAPI.
- Logging: `tracing` with `RUST_LOG` env var (e.g., `vpr=debug`).
- Error handling: tonic `Status` for gRPC, axum `StatusCode` for REST; internal errors logged with `tracing::error!`.
- File I/O: Direct `std::fs` operations with `serde_json`/`serde_yaml` for patient data; no database layer.
- Git versioning: Each patient directory is a Git repo; commits signed with X.509 certificates from author.signature.
  - Clinical template: `templates/clinical/` directory copied to new patient clinical dirs; validated at startup.

Runtime configuration and environment variables

- Resolve environment variables **once at process startup** (or CLI startup) and pass configuration down.
  - Create a `vpr_core::CoreConfig` (see `crates/core/src/config.rs`) in the binary entrypoints:
    - `src/main.rs` (vpr-run)
    - `crates/api-grpc/src/main.rs` (standalone gRPC)
    - `crates/api-rest/src/main.rs` (standalone REST)
    - `crates/cli/src/main.rs` (CLI)
  - Typical env inputs: `PATIENT_DATA_DIR`, `VPR_CLINICAL_TEMPLATE_DIR`, `RM_SYSTEM_VERSION`, `VPR_NAMESPACE`.
  - Use the helpers in `crates/core/src/config.rs` to resolve/validate template and parse the RM version.
- `crates/core` (vpr-core) must **not** read environment variables during operations.
  - Do not call `std::env::var` in core service methods or helpers.
  - Prefer constructors like `ClinicalService::new(Arc<CoreConfig>)` for uninitialised state, or `ClinicalService::with_id(Arc<CoreConfig>, Uuid)` for initialised state. Same for `DemographicsService::new(Arc<CoreConfig>)`.
  - This avoids rare-but-real process-wide env races and keeps behaviour consistent within a request.

Defensive programming (clinical safety)

- Treat defensive programming as a non-negotiable requirement.
- Validate inputs and configuration early and fail fast (arguments, resolved startup configuration, parsed identifiers) before doing filesystem/Git side effects.
- Prefer bounded work over unbounded behaviour (retry limits, traversal depth, file counts/sizes, timeouts where applicable).
- Avoid silent fallbacks and “best effort” behaviour in core logic: return a typed error when something is invalid.
- Avoid `panic!`/`expect()` on paths influenced by inputs or environment; reserve them for internal invariants only.
- When partial work has occurred, attempt cleanup/rollback and do not ignore cleanup failures.- **Strong static typing**: Leverage Rust's type system to encode invariants and prevent errors at compile time. Use wrapper types to represent validated data (e.g., `ShardableUuid` for canonical UUIDs, `Author` for validated commit authors). Avoid stringly-typed data, primitive obsession, and runtime checks where types can express constraints. Prefer newtype patterns and distinct types over raw strings, integers, or booleans when domain concepts have specific rules.- **Formatting**: All Rust code MUST follow `cargo fmt` standards. Before completing any changes, run `cargo fmt` on the workspace. Do not commit code that fails `cargo fmt --check`. The project uses `rustfmt.toml` for consistent formatting enforced by pre-commit hooks.
- Spelling: Use British English (en-GB) for documentation and other prose (mdBook pages, README, Rustdoc/comments).
- Documentation style:
  - Use **Rustdoc** (doc comments) with standard section headings.
  - For functions/methods (including private helpers), include clear `# Arguments`, `# Returns`, and `# Errors` sections **when applicable**.
    - Include `# Arguments` for all methods with parameters (public or private), documenting what each parameter represents.
    - Include `# Returns` for all methods that return non-unit values (public or private), describing what is returned.
    - If there are no arguments/meaningful return value/no error conditions to document, omit the empty section.
    - For `# Errors`, prefer a short, grouped bullet list describing the _conditions_ under which an error is returned (not an exhaustive list of enum variants).
      - Use the form: `Returns <ErrorType> if:` then `- ...` bullets.
      - Group by category when helpful (validation/config, filesystem I/O, serialisation, Git, crypto).
  - For each module, start the file with `//!` module-level Rustdoc that outlines what the module does and what it is intended to do.
  - **Documentation examples**: In Rust, documentation examples are executable doctests and should be used deliberately, not everywhere by default. Examples are encouraged when they clarify lifecycle rules, state transitions, ordering constraints, or non-obvious correct usage, as they act as part of the correctness and safety contract of the code. Avoid adding examples to trivial helpers or internal plumbing where the signature is self-explanatory. Prefer a small number of minimal, focused examples that encode important invariants rather than repetitive or decorative usage snippets.
- Imports and naming:
  - Prefer adding clear `use` imports (for example, `use crate::uuid::ShardableUuid;`) rather than repeating long paths like `crate::...` throughout the file.
  - Prefer calling imported items directly (e.g. `copy_dir_recursive(...)`) instead of qualifying call sites with `crate::copy_dir_recursive(...)`.
    - Exception: keep fully-qualified paths only when needed to disambiguate names.
  - For constants, prefer importing the specific items by name (for example `use crate::constants::{EHR_STATUS_FILENAME, LATEST_RM};`) so call sites don’t need `constants::...` prefixes.
  - Avoid glob imports (`use crate::foo::*;`) unless there is a strong reason.
  - Keep imports scoped to what the file uses; remove unused imports to satisfy clippy `-D warnings`.
  - If two imports would conflict, use explicit renaming (`use crate::thing::Type as ThingType;`) rather than falling back to fully-qualified paths everywhere.
- **Architecture boundaries**:
  - `core`: ONLY file/folder/git operations (ClinicalService, DemographicsService, data persistence)
  - `api-shared`: Shared API utilities (HealthService, auth, protobuf types)
  - `api-grpc`: gRPC-specific concerns (service implementation, interceptors)
  - `api-rest`: REST-specific concerns (HTTP endpoints, JSON handling)
  - `main.rs`: Startup validation (patient_data, clinical-template dirs), runtime constants, service orchestration

Testing boundaries

- Test where the rule lives:
  - If a function _implements validation rules_ (for example `Author::validate_commit_author`), write exhaustive unit tests for each failure mode and a success case.
  - If a function merely _calls validation_ (for example `ClinicalService::initialise` calling `author.validate_commit_author()?`), write only wiring tests:
    - validation errors are returned unchanged,
    - no side effects occur when validation fails.
- Prefer true unit tests (no filesystem/Git/network) where possible; use TempDir-backed tests only for integration-level behaviour (directory layout, Git repo creation, template copying).

Change policy and safety

- Prefer minimal, well-scoped PRs updating single crates or modules.
- Run `./scripts/check-all.sh` before proposing changes; fix clippy warnings.
- When changing protos: Update `crates/api-shared/vpr.proto`, regenerate with `cargo build`.
- Patient data paths: Hardcoded sharding logic in `core`; avoid changing without testing directory traversal in `list_patients`.
- Environment config: Env vars are read in binaries/CLI at startup to build `CoreConfig`; avoid adding env reads to `crates/core`.
- Proto fields: Some fields (e.g., national_id) present but unused in current implementation.

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
- Clinical template validation: `clinical-template/` must exist and contain files; clinical init copies it recursively.

If unsure, ask for clarification and provide a short plan: files to change, tests to add, and commands you will run to validate.

If you'd like I can expand any section (e.g., CI, proto build details, or example PR checklist).
