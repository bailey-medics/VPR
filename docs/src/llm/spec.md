# LLM Specification (Draft)

## Purpose and Scope

- Define how LLM tooling supports the VPR project while respecting safety, auditability, and architecture boundaries.
- Focus on assistant-driven code/docs changes and developer workflows; avoid introducing runtime LLM features unless explicitly approved.
- Keep this spec aligned with [docs/src/llm/copilot-instructions.md](./copilot-instructions.md) (canonical guidance for AI contributors).

## System Context

- VPR is a Rust Cargo workspace delivering dual gRPC/REST services plus a CLI over a file-based, Git-versioned patient record store.
- Core data operations live in `crates/core`; transports live in `crates/api-grpc` and `crates/api-rest`; shared proto/auth/health in `crates/api-shared`; certificate utilities in `crates/certificates`; CLI in `crates/cli`.
- Patient data is stored on disk, sharded by UUID under `patient_data/`, with separate clinical and demographics repos per patient and Git history for audit.

## LLM Responsibilities (assistant mode)

- Follow canonical contributor instructions: defensive programming, British English docs, architecture boundaries, startup config resolution in binaries only.
- Generate scoped changes with clear rationale, minimal blast radius, and accompanying tests when behaviour changes.
- Keep docs consistent across mdBook sources (`docs/src/**`) and README; prefer linking to canonical sources instead of duplicating.

## Data and Storage Invariants

- Sharded directories: `patient_data/clinical/<s1>/<s2>/<uuid>/` and `patient_data/demographics/<s1>/<s2>/<uuid>/` (s1/s2 are first 4 hex chars).
- Clinical repo seeded from validated clinical template directory (no symlinks; depth/size limits enforced); demographics repo holds FHIR-like `patient.json`.
- Git repos per patient with signed commits (ECDSA P-256) where configured; single branch `main`.
- Clinical `ehr_status` links to demographics via external reference.

## API Surfaces (high level)

- gRPC service (tonic): health, patient creation, patient listing; API key interceptor expected on gRPC.
- REST service (axum/utoipa): mirrors gRPC behaviour; Swagger/OpenAPI exposed; currently open by default unless otherwise configured.
- Health endpoints on both transports; reflection optional for gRPC.

## Configuration and Startup

- Env resolved once at startup in binaries/CLI, then passed via `CoreConfig`: `PATIENT_DATA_DIR`, `VPR_CLINICAL_TEMPLATE_DIR`, `RM_SYSTEM_VERSION`, `VPR_NAMESPACE`, API key, bind addresses, reflection flag, dev guard for destructive CLI.
- Startup flow (vpr-run): validate patient_data and template dirs, ensure shard subdirs exist, build config, launch REST and gRPC concurrently with `tokio::join`.

## Safety and Quality Bar

- Fail fast on invalid config/inputs; avoid panics on input-driven paths; no silent fallbacks.
- Respect architecture boundaries: `crates/core` must not read env; transports handle auth and request wiring.
- Add tests where rules live; wiring tests ensure errors propagate and side effects do not occur on failure.
- Use British English in prose and Rustdoc; prefer module-level `//!` docs and function docs with `# Arguments`, `# Returns`, `# Errors` when applicable.

## Security Expectations (for LLM-driven changes)

- Default to least privilege and minimise new attack surface; do not introduce new network listeners, env reads in `crates/core`, or unsafe defaults.
- Keep authentication posture aligned with project decisions: API key for gRPC (and REST when enabled); defer mTLS or other mechanisms to explicit user approval.
- Handle secrets safely in code and docs: avoid logging API keys, certificates, or patient identifiers; redact in logs and examples.
- Preserve commit-signing and integrity paths: do not weaken signature requirements or verification flows without explicit agreement.
- Avoid introducing PHI (Protected Health Information) into logs, test fixtures, or examples; prefer synthetic/non-identifying data.
- When adding dependencies, prefer well-maintained crates with permissive licenses; avoid unsafe code unless strictly necessary and justified.

## Build, Test, and Tooling

- Primary check pipeline: `./scripts/check-all.sh` (fmt, clippy -D warnings, check, test).
- Docker dev: `docker compose -f compose.dev.yml up --build` or `just start-dev`; health via grpcurl and REST /health.
- Proto changes: edit `crates/api-shared/vpr.proto`, rebuild to regenerate bindings.

## Open Questions / Next Decisions

- Confirm scope: Is LLM limited to contributor assistance, or will user-facing LLM features (summaries/search) be added? If runtime features are desired, specify data access boundaries, PHI handling, and auditing requirements.
- Define authentication posture for REST (API key, mTLS, or other) to align with gRPC.
- Clarify expected commit-signing defaults (enforce vs optional) and how LLM-generated changes should treat signing in CI/local dev.
