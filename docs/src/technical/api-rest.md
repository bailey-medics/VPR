# REST API

The VPR REST API provides HTTP/JSON access to patient record operations with OpenAPI documentation.

## Overview

The REST API is built using:
- **axum** 0.7 - Rust web framework
- **utoipa** 4.x - OpenAPI specification and Swagger UI generation
- **JSON** - Request and response format

## Base URL

```
http://localhost:3000
```

## Interactive Documentation

Swagger UI is available at:

```
http://localhost:3000/swagger-ui/
```

This provides interactive API documentation where you can test endpoints directly.

## Authentication

Currently, the REST API does not require authentication (unlike the gRPC API). This is subject to change in future versions.

## Available Endpoints

### Health Check

- **`GET /health`** - Returns service health status

### Patient Management

- **`POST /patients/full`** - Creates complete patient record (demographics, clinical, coordination)

### Demographics

- **`POST /demographics`** - Initialises new demographics repository
- **`PUT /demographics/:id`** - Updates patient demographics

### Clinical

- **`POST /clinical`** - Initialises new clinical repository
- **`POST /clinical/:id/link`** - Links clinical repository to demographics
- **`POST /clinical/:id/letters`** - Creates new letter
- **`GET /clinical/:id/letters/:letter_id`** - Retrieves letter content

### Coordination

- **`POST /coordination`** - Initialises new coordination repository

## Example Usage with curl

### Create Full Patient Record

```bash
curl -X POST http://localhost:3000/patients/full \
  -H 'Content-Type: application/json' \
  -d '{
    "given_names": ["Emily"],
    "last_name": "Davis",
    "birth_date": "1985-03-20",
    "author": {
      "name": "Dr. Robert Brown",
      "email": "robert.brown@example.com",
      "role": "Clinician",
      "registrations": [{"authority": "GMC", "number": "5555555"}],
      "care_location": "City General Hospital"
    }
  }'
```

Response:
```json
{
  "demographics_uuid": "d4c6547ee14a4255a568aa66d7335561",
  "clinical_uuid": "a701c3a94bf34a939d831d6183a78734",
  "coordination_uuid": "da7e89a2a51647db89430dc3a781abb0"
}
```

### Initialise Demographics

```bash
curl -X POST http://localhost:3000/demographics \
  -H 'Content-Type: application/json' \
  -d '{
    "author": {
      "name": "Dr. Jane Smith",
      "email": "jane.smith@example.com",
      "role": "Clinician",
      "registrations": [{"authority": "GMC", "number": "1234567"}],
      "care_location": "St. Mary'\''s Hospital"
    }
  }'
```

### Update Demographics

```bash
curl -X PUT http://localhost:3000/demographics/d4c6547ee14a4255a568aa66d7335561 \
  -H 'Content-Type: application/json' \
  -d '{
    "given_names": ["Emily", "Rose"],
    "last_name": "Davis",
    "birth_date": "1985-03-20"
  }'
```

### Initialise Clinical Repository

```bash
curl -X POST http://localhost:3000/clinical \
  -H 'Content-Type: application/json' \
  -d '{
    "author": {
      "name": "Dr. Robert Brown",
      "email": "robert.brown@example.com",
      "role": "Clinician",
      "care_location": "City Hospital"
    }
  }'
```

### Link Clinical to Demographics

```bash
curl -X POST http://localhost:3000/clinical/a701c3a94bf34a939d831d6183a78734/link \
  -H 'Content-Type: application/json' \
  -d '{
    "demographics_uuid": "d4c6547ee14a4255a568aa66d7335561",
    "author": {
      "name": "Dr. Brown",
      "email": "brown@example.com",
      "role": "Clinician",
      "care_location": "City Hospital"
    },
    "namespace": "example.org"
  }'
```

### Create Letter

```bash
curl -X POST http://localhost:3000/clinical/a701c3a94bf34a939d831d6183a78734/letters \
  -H 'Content-Type: application/json' \
  -d '{
    "content": "# Consultation Note\n\nPatient presented with hypertension.",
    "author": {
      "name": "Dr. Sarah Johnson",
      "email": "sarah.johnson@example.com",
      "role": "Clinician",
      "registrations": [{"authority": "GMC", "number": "7654321"}],
      "care_location": "GP Clinic"
    }
  }'
```

Response:
```json
{
  "timestamp_id": "20260125T125621.563Z-8d263432-d614-4d51-8611-22d365b6afa7"
}
```

### Read Letter

```bash
curl http://localhost:3000/clinical/a701c3a94bf34a939d831d6183a78734/letters/20260125T125621.563Z-8d263432-d614-4d51-8611-22d365b6afa7
```

Response:
```json
{
  "body_content": "# Consultation Note\n\nPatient presented with hypertension.",
  "rm_version": "1.0.4",
  "composer_name": "Dr. Sarah Johnson",
  "composer_role": "Clinician",
  "start_time": "2026-01-25T12:56:21.563Z",
  "clinical_lists": [...]
}
```

### Initialise Coordination

```bash
curl -X POST http://localhost:3000/coordination \
  -H 'Content-Type: application/json' \
  -d '{
    "clinical_uuid": "a701c3a94bf34a939d831d6183a78734",
    "author": {
      "name": "Dr. Brown",
      "email": "brown@example.com",
      "role": "Clinician",
      "care_location": "City Hospital"
    }
  }'
```

## Request/Response Formats

### Author Object

All mutation endpoints accept an `author` object:

```json
{
  "author": {
    "name": "Dr. John Smith",
    "email": "john.smith@example.com",
    "role": "Clinician",
    "registrations": [
      {
        "authority": "GMC",
        "number": "1234567"
      }
    ],
    "care_location": "City General Hospital",
    "signature": "optional-pem-encoded-signature"
  }
}
```

### Error Responses

Errors return appropriate HTTP status codes with JSON error details:

```json
{
  "error": "Error message",
  "details": "Additional context"
}
```

Common status codes:
- `200 OK` - Success
- `201 Created` - Resource created
- `400 Bad Request` - Invalid input
- `404 Not Found` - Resource not found
- `500 Internal Server Error` - Server error

## OpenAPI Specification

The OpenAPI specification is automatically generated from code annotations and available at:

```
http://localhost:3000/api-doc/openapi.json
```

## Server Configuration

The REST server runs on port 3000 by default. Configuration via environment variables:

- `VPR_REST_ADDR` - Server bind address (default: `0.0.0.0:3000`)
- `RUST_LOG` - Logging configuration

## Implementation

The REST API is implemented in [`crates/api-rest/src/main.rs`](../../crates/api-rest/src/main.rs).

Key characteristics:
- **Path parameter extraction** - Uses axum `Path` extractor for UUIDs
- **JSON payloads** - Uses axum `Json` extractor for request bodies
- **Author construction** - Helper function builds `Author` from JSON
- **Error handling** - Maps errors to HTTP status codes
- **OpenAPI annotations** - Each handler annotated with `#[utoipa::path]`

## Comparison with gRPC API

| Feature | REST API | gRPC API |
|---------|----------|----------|
| Protocol | HTTP/JSON | HTTP/2 + Protocol Buffers |
| Performance | Good | Excellent |
| Authentication | None (currently) | API key required |
| Type Safety | Runtime validation | Compile-time |
| Documentation | OpenAPI/Swagger | Protocol Buffer IDL |
| Binary Data | Base64 encoding | Native bytes |
| Streaming | Not supported | Supported |

## Future Enhancements

Planned additions:
- Authentication and authorization
- Additional endpoints for messaging operations
- File upload support for letter attachments
- Pagination for list operations
- Filtering and search capabilities

## Related Documentation

- [gRPC API](api-grpc.md)
- [CLI Commands](../cli.md)
- [OpenAPI Specification](http://localhost:3000/api-doc/openapi.json)
