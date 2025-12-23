<!--
Guidance for AI coding agents working on the VPR repository.
Focus: be pragmatic, reference concrete files, and keep changes minimal and well-tested.
-->
# VPR — AI contributor notes

These notes are for automated coding agents and should be short, concrete, and codebase-specific.

Overview
- Purpose: VPR is a file-based patient record system with Git-like versioning, built as a Rust Cargo workspace. It provides dual gRPC and REST APIs for health checks and patient creation. The system stores patient data as JSON files in a sharded directory structure under `patient_data/`.
- Key crates:
  - `crates/core` — Core service logic implementing the VPR gRPC service (see `crates/core/src/lib.rs`).
  - `crates/api-proto` — Protobuf definitions and generation (canonical proto at `crates/api-proto/vpr.proto`).
  - `crates/api-grpc` — gRPC server setup and re-exports (entry via `src/main.rs`).
  - `crates/api-rest` — REST API components (integrated into main binary).
- Main binary: `vpr-run` (defined in root `Cargo.toml`), runs both gRPC (port 50051) and REST (port 3000) servers concurrently using tokio::join.

Important files to reference
- `src/main.rs` — Main binary that starts both gRPC and REST servers, with OpenAPI/Swagger UI.
- `crates/core/src/lib.rs` — Implements `VprService` with patient creation (sharded JSON storage) and listing (directory traversal).
- `crates/api-proto/vpr.proto` — Canonical protobuf definitions for VPR service.
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
- Protobufs: Canonical proto in `crates/api-proto/vpr.proto`; generated Rust in `api_proto::pb` via build script.
- Service wiring: `crates/api-grpc` re-exports `VprService` from `core`; main.rs uses `api_grpc::VprService`.
- Patient storage: Sharded under `PATIENT_DATA_DIR` (env var, defaults to `/patient_data`): `<s1>/<s2>/<32hex-uuid>/demographics.json` where s1/s2 are first 4 hex chars.
- APIs: Dual gRPC/REST with identical functionality; REST uses axum, utoipa for OpenAPI.
- Logging: `tracing` with `RUST_LOG` env var (e.g., `vpr=debug`).
- Error handling: tonic `Status` for gRPC, axum `StatusCode` for REST; internal errors logged with `tracing::error!`.
- File I/O: Direct `std::fs` operations with `serde_json` for patient data; no database layer yet.

Change policy and safety
- Prefer minimal, well-scoped PRs updating single crates or modules.
- Run `./scripts/check-all.sh` before proposing changes; fix clippy warnings.
- When changing protos: Update `crates/api-proto/vpr.proto`, regenerate with `cargo build`.
- Patient data paths: Hardcoded sharding logic in `core`; avoid changing without testing directory traversal in `list_patients`.
- Environment config: Use env vars like `VPR_ADDR`, `PATIENT_DATA_DIR`; defaults in code.

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

If unsure, ask for clarification and provide a short plan: files to change, tests to add, and commands you will run to validate.

---
If you'd like I can expand any section (e.g., CI, proto build details, or example PR checklist).
