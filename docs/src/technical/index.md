# Technical

See [Design Decisions](design-decisions.md) for more information on architecture and design choices.

## Containers

Docker

## Language

Rust

## gRPC

via tonic

To start the grpcui viewer, just run:

```bash
j g
```

## REST API

```text
http://localhost:3000/swagger-ui/
```

## Linting

Rust Clippy
markdownlint

## Spelling

cspell

## Pre-commit

pre-commit

## Crate Separation

The VPR project uses a modular crate structure to maintain clear separation of concerns and enforce architectural boundaries:

- **`crates/core`**: Contains pure data operations only. Handles file/folder management, patient CRUD, and Git-like versioning. No API concerns (authentication, HTTP/gRPC servers, service interfaces).

- **`crates/api-shared`**: Shared utilities and definitions for both APIs. Includes Protobuf types, HealthService, and authentication utilities.

- **`crates/api-grpc`**: gRPC-specific implementation. Uses VprService with authentication interceptors and tonic integration, delegating to PatientService from core.

- **`crates/api-rest`**: REST-specific implementation. Provides HTTP endpoints, OpenAPI/Swagger UI via axum and utoipa, delegating to PatientService from core.

- **`crates/cli`**: Command-line interface. Provides CLI commands (e.g., `vpr hi`, `vpr list`) that interact with PatientService from core.

This separation ensures that data logic remains isolated from API specifics, making the codebase maintainable and testable.
