# gRPC API

The VPR gRPC API provides high-performance, type-safe access to all patient record operations.

## Overview

The gRPC API is built using:
- **tonic** 0.12 - Rust gRPC framework
- **Protocol Buffers** - For message serialization
- **Authentication** - API key-based authentication via x-api-key header

## Service Definition

The API is defined in [`crates/api-shared/vpr.proto`](../../crates/api-shared/vpr.proto).

### Service: `VPR`

All RPC methods are grouped under the `vpr.v1.VPR` service.

## Authentication

All requests require an `x-api-key` header:

```bash
grpcurl -H 'x-api-key: YOUR_API_KEY' localhost:50051 vpr.v1.VPR/Health
```

The API key is configured via the `API_KEY` environment variable.

## Available RPCs

### Health Check

- **`Health`** - Returns service health status

### Patient Management

- **`CreatePatient`** - Creates a new patient record (legacy)
- **`ListPatients`** - Lists all patients
- **`InitialiseFullRecord`** - Creates complete patient record (demographics, clinical, coordination)

### Demographics

- **`InitialiseDemographics`** - Initialises new demographics repository
- **`UpdateDemographics`** - Updates patient demographics (given names, last name, birth date)

### Clinical

- **`InitialiseClinical`** - Initialises new clinical repository
- **`LinkToDemographics`** - Links clinical repository to demographics via EHR status
- **`NewLetter`** - Creates new clinical letter with markdown content
- **`ReadLetter`** - Retrieves letter content and metadata
- **`NewLetterWithAttachments`** - Creates letter with binary file attachments
- **`GetLetterAttachments`** - Retrieves letter attachments (metadata and binary content)

### Coordination

- **`InitialiseCoordination`** - Initialises new coordination repository
- **`CreateThread`** - Creates messaging thread with participants
- **`AddMessage`** - Adds message to existing thread
- **`ReadCommunication`** - Reads thread with ledger and all messages
- **`UpdateCommunicationLedger`** - Updates thread participants, status, visibility
- **`UpdateCoordinationStatus`** - Updates coordination lifecycle state and flags

## Example Usage with grpcurl

### Create Full Patient Record

```bash
grpcurl -plaintext -import-path crates/api-shared -proto vpr.proto \
  -d '{
    "given_names": ["Emily"],
    "last_name": "Davis",
    "birth_date": "1985-03-20",
    "author_name": "Dr. Robert Brown",
    "author_email": "robert.brown@example.com",
    "author_role": "Clinician",
    "author_registrations": [{"authority": "GMC", "number": "5555555"}],
    "care_location": "City General Hospital"
  }' \
  -H 'x-api-key: YOUR_API_KEY' \
  localhost:50051 vpr.v1.VPR/InitialiseFullRecord
```

### Create Letter

```bash
grpcurl -plaintext -import-path crates/api-shared -proto vpr.proto \
  -d '{
    "clinical_uuid": "a701c3a94bf34a939d831d6183a78734",
    "author_name": "Dr. Sarah Johnson",
    "author_email": "sarah.johnson@example.com",
    "author_role": "Clinician",
    "author_registrations": [{"authority": "GMC", "number": "7654321"}],
    "care_location": "GP Clinic",
    "content": "# Consultation\\n\\nPatient presented with hypertension."
  }' \
  -H 'x-api-key: YOUR_API_KEY' \
  localhost:50051 vpr.v1.VPR/NewLetter
```

### Create Letter with Attachments

Binary attachments are sent as base64-encoded bytes:

```bash
# Encode file to base64
base64 -i /path/to/file.pdf

grpcurl -plaintext -import-path crates/api-shared -proto vpr.proto \
  -d '{
    "clinical_uuid": "a701c3a94bf34a939d831d6183a78734",
    "author_name": "Dr. Chen",
    "author_email": "chen@example.com",
    "author_role": "Clinician",
    "care_location": "Hospital Lab",
    "attachment_files": ["<base64_content>"],
    "attachment_names": ["lab_results.pdf"]
  }' \
  -H 'x-api-key: YOUR_API_KEY' \
  localhost:50051 vpr.v1.VPR/NewLetterWithAttachments
```

### Create Communication Thread

```bash
grpcurl -plaintext -import-path crates/api-shared -proto vpr.proto \
  -d '{
    "coordination_uuid": "da7e89a2a51647db89430dc3a781abb0",
    "author_name": "Dr. Brown",
    "author_email": "brown@example.com",
    "author_role": "Clinician",
    "care_location": "City Hospital",
    "participants": [
      {"id": "a701c3a94bf34a939d831d6183a78734", "name": "Dr. Brown", "role": "clinician"},
      {"id": "d4c6547ee14a4255a568aa66d7335561", "name": "Emily Davis", "role": "patient"}
    ],
    "initial_message_body": "Consultation scheduled.",
    "initial_message_author": {
      "id": "a701c3a94bf34a939d831d6183a78734",
      "name": "Dr. Brown",
      "role": "clinician"
    }
  }' \
  -H 'x-api-key: YOUR_API_KEY' \
  localhost:50051 vpr.v1.VPR/CreateThread
```

## Message Types

Key message types defined in the protocol:

### `Author Registration`
```protobuf
message AuthorRegistration {
  string authority = 1;  // e.g., "GMC", "NMC"
  string number = 2;     // Registration number
}
```

### `Message Author`
```protobuf
message MessageAuthor {
  string id = 1;    // UUID
  string name = 2;  // Display name
  string role = 3;  // clinician, patient, system, etc.
}
```

### Lifecycle States

Coordination lifecycle states:
- `active` - Operational and accepting updates
- `suspended` - Temporarily inactive
- `closed` - Permanently closed

Thread statuses:
- `open` - Active communication
- `closed` - Concluded communication
- `archived` - Historical record

Sensitivity levels:
- `standard` - Normal clinical communication
- `confidential` - Elevated privacy
- `restricted` - Highest privacy level

## Server Configuration

The gRPC server runs on port 50051 by default. Configuration via environment variables:

- `VPR_ADDR` - Server bind address (default: `0.0.0.0:50051`)
- `API_KEY` - Required API key for authentication
- `VPR_ENABLE_REFLECTION` - Enable gRPC reflection (default: `false`)
- `RUST_LOG` - Logging configuration

## Implementation

The gRPC service is implemented in [`crates/api-grpc/src/service.rs`](../../crates/api-grpc/src/service.rs).

Key characteristics:
- **Authentication interceptor** - Validates API key on all requests
- **Author construction** - Builds `Author` objects from proto fields
- **Error handling** - Maps Rust errors to gRPC status codes
- **File handling** - Writes attachments to temp directory, uses FilesService, cleans up
- **Type conversions** - Converts string enums to Rust enums (AuthorRole, ThreadStatus, etc.)

## Error Handling

gRPC status codes used:
- `OK` - Success
- `UNAUTHENTICATED` - Invalid or missing API key
- `INVALID_ARGUMENT` - Invalid input parameters
- `NOT_FOUND` - Resource not found
- `INTERNAL` - Server error

Error messages include descriptive details for debugging.

## Related Documentation

- [REST API](api-rest.md)
- [CLI Commands](../cli.md)
- [Protocol Buffer Definition](../../crates/api-shared/vpr.proto)
