# Technical

See [Design Decisions](design-decisions.md) for more information on architecture and design choices.

## Containers

Docker

## Language

Rust

## APIs

VPR provides two API interfaces for accessing patient records:

### gRPC API

High-performance, type-safe API using Protocol Buffers and tonic.

- Port: `50051`
- Protocol: HTTP/2 + Protocol Buffers
- Authentication: API key via `x-api-key` header
- See [gRPC API Documentation](api-grpc.md)

To start the grpcui viewer:

```bash
j g
```

### REST API

HTTP/JSON API with OpenAPI documentation and Swagger UI.

- Port: `3000`
- Protocol: HTTP/JSON  
- Interactive documentation: `http://localhost:3000/swagger-ui/`
- See [REST API Documentation](api-rest.md)

## Linting

Rust Clippy
markdownlint

## Spelling

cspell

## Pre-commit

pre-commit

## Crate Separation

The VPR project uses a modular crate structure to maintain clear separation of concerns and enforce architectural boundaries:

### Core Crates

- **`crates/core`** (`vpr-core`): Contains pure data operations only. Handles file/folder management, patient repositories (clinical, demographics, coordination), and Git-based versioning. No API concerns. Provides the foundational services: `ClinicalService`, `DemographicsService`, `CoordinationService`, `PatientService`.

- **`crates/files`** (`vpr-files`): Content-addressed file storage for binary attachments. Implements SHA-256-based immutable file storage with two-level sharding. Used by clinical repository for letter attachments.

- **`crates/uuid`** (`vpr-uuid`): UUID generation and sharding utilities. Provides `ShardableUuid` for creating two-level sharded directory structures.

- **`crates/fhir`**: FHIR-aligned data types and enums. Provides `MessageAuthor`, `AuthorRole`, `ThreadStatus`, `SensitivityLevel`, `LifecycleState` for care coordination.

- **`crates/openehr`**: OpenEHR data structures and validation. Used for clinical content modeling.

- **`crates/certificates`** (`vpr-certificates`): X.509 certificate generation and validation for professional registrations. Supports ECDSA P-256 cryptographic signing.

### API Crates

- **`crates/api-shared`** (`api-shared`): Shared utilities and definitions for both APIs. Includes Protocol Buffer definitions (`vpr.proto`), message types, and common authentication utilities.

- **`crates/api-grpc`** (`api-grpc`): gRPC-specific implementation. Uses `VprService` with authentication interceptors and tonic integration. All RPCs delegate to services from `vpr-core`.

- **`crates/api-rest`** (`api-rest`): REST-specific implementation. Provides HTTP endpoints with OpenAPI/Swagger UI via axum and utoipa. All handlers delegate to services from `vpr-core`.

### CLI and Deployment

- **`crates/cli`** (`vpr-cli`): Command-line interface. Provides comprehensive CLI commands for all patient record operations. Directly uses services from `vpr-core`.

- **`src/main.rs`** (`vpr-run`): Deployment binary that runs both gRPC and REST servers concurrently.

This separation ensures that data logic remains isolated from API specifics, making the codebase maintainable, testable, and allowing multiple deployment configurations from the same core.
