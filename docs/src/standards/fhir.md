# FHIR Standard

## Overview

Fast Healthcare Interoperability Resources (FHIR) is a modern healthcare data exchange standard developed by HL7 International. Released in 2014, FHIR combines the best features of HL7 v2, v3, and CDA while leveraging web technologies (REST, JSON, OAuth) to provide a practical, implementer-friendly approach to health data interoperability.

### Core Concepts

**Resources:**

FHIR defines ~150 modular "resources" representing healthcare concepts:

- **Clinical**: Patient, Observation, Condition, Procedure, MedicationStatement
- **Administrative**: Encounter, Practitioner, Organization, Location
- **Financial**: Claim, Coverage, PaymentNotice
- **Workflow**: Task, Appointment, ServiceRequest
- **Infrastructure**: Bundle, OperationOutcome, CapabilityStatement

Each resource:

- Has a defined structure (elements and data types)
- Can be represented as JSON, XML, or RDF
- Includes human-readable narrative
- Supports extensibility via extensions
- Has a defined lifecycle and versioning model

**RESTful API:**

FHIR uses HTTP for all interactions:

- `GET /Patient/123` - Read a patient
- `POST /Observation` - Create an observation
- `PUT /Condition/456` - Update a condition
- `DELETE /MedicationStatement/789` - Remove (or mark inactive)
- `GET /Patient?name=Smith` - Search for patients

**Profiles and Implementation Guides:**

FHIR can be constrained for specific use cases:

- **Profiles**: Constrain resources for particular jurisdictions or domains
- **Implementation Guides**: Collections of profiles, value sets, and documentation
- **Examples**: US Core, UK Core, International Patient Summary (IPS)

**Terminology Integration:**

FHIR supports standard terminologies:

- CodeableConcept data type for coded values
- ValueSets for allowed codes
- ConceptMaps for code translation
- Built-in support for SNOMED CT, LOINC, RxNorm, ICD-10, etc.

**Extensions:**

FHIR allows extending resources without breaking compatibility:

- Standard extensions (e.g., patient ethnicity, race)
- Local extensions for organization-specific needs
- Extensions can be profiled and constrained

---

## Problems FHIR Solves

### 1. API-First Health Data Exchange

**Problem:** Legacy standards (HL7 v2, CDA) weren't designed for modern web APIs, making integration complex and expensive.

**Solution:** FHIR uses RESTful HTTP APIs that web developers understand. OAuth 2.0 for security, JSON for data format, and standard HTTP verbs make integration straightforward.

### 2. Implementation Complexity

**Problem:** HL7 v3 and CDA were powerful but extremely complex, leading to inconsistent implementations and high development costs.

**Solution:** FHIR prioritizes the "80% use case" with simple, practical designs. Complex scenarios are supported but don't burden simple implementations.

### 3. Granular Data Access

**Problem:** Document-based standards (CDA) require exchanging entire documents when only specific data elements are needed.

**Solution:** FHIR resources are granular (e.g., single Observation for one vital sign). Systems retrieve only what they need, reducing bandwidth and processing overhead.

### 4. Mobile and Consumer Health

**Problem:** Legacy standards weren't designed for patient-facing applications or mobile devices.

**Solution:** FHIR's lightweight JSON format, RESTful APIs, and OAuth security work naturally with mobile apps and patient portals. SMART on FHIR enables app ecosystems.

### 5. Real-Time Clinical Decision Support

**Problem:** Batch-oriented standards delay clinical decision support until data is processed and stored.

**Solution:** FHIR's API model supports real-time CDS Hooks—contextual cards that appear during clinical workflow without disrupting the EHR.

### 6. Data Heterogeneity

**Problem:** Healthcare data comes in many forms (structured, narrative, images, documents), and legacy standards handle some better than others.

**Solution:** FHIR resources accommodate:

- Structured coded data (Observation with LOINC codes)
- Narrative text (DomainResource.text)
- Binary data (DocumentReference, Media)
- Mixed content (DiagnosticReport with narrative + structured results)

### 7. International Adoption

**Problem:** Different countries have different healthcare models, terminologies, and regulations, making global standards difficult.

**Solution:** FHIR's profiling mechanism allows local adaptation while maintaining core compatibility. US Core, UK Core, Australian Base, and others all build on the same foundation.

---

## How FHIR is Normally Used in Digital Health

### 1. Health Information Exchange (HIE)

FHIR enables data sharing across organizations:

- **Query-based exchange**: Pull patient data from other systems when needed
- **Subscription-based exchange**: Get notified when patient data changes
- **Bulk data export**: Extract large datasets for research or migration
- **National networks**: CommonWell, Carequality (US), Summary Care Record (UK)

### 2. Patient Access to Health Records

FHIR powers patient-facing applications:

- **Patient portals**: View records, request appointments, message providers
- **Mobile health apps**: Apple Health, Google Fit integration
- **SMART on FHIR apps**: Patient selects apps that access their EHR data
- **Blue Button 2.0**: US Medicare beneficiaries download their claims data

### 3. Provider Access to External Data

FHIR brings outside data into clinical workflow:

- **CDS Hooks**: Real-time clinical decision support during ordering
- **SMART on FHIR**: Clinician-facing apps launch from within EHR
- **Payer data exchange**: Claims history informs clinical care
- **Social determinants**: Community resource directories, housing, food access

### 4. Clinical Research and Registries

FHIR supports research data collection:

- **HL7 FHIR Bulk Data**: Extract cohorts for research studies
- **REDCap on FHIR**: Capture study data in FHIR format
- **Quality registries**: Automated reporting to cancer, cardiac registries
- **Phenotyping**: Identify eligible patients for trials

### 5. Population Health and Value-Based Care

FHIR enables population-level analytics:

- **Risk stratification**: Identify high-risk patients for intervention
- **Gap closure**: Find patients missing preventive care
- **Care coordination**: Track care plan execution across providers
- **Quality measurement**: Automated HEDIS, CQM reporting

### 6. Public Health Reporting

FHIR modernizes public health surveillance:

- **Electronic case reporting (eCR)**: Automated notifiable disease reporting
- **Immunization forecasting**: Calculate due/overdue vaccines
- **Lab result reporting**: ELR via FHIR Observation
- **COVID-19 reporting**: Vaccine administration, case reports, lab results

### 7. Payer-Provider Data Exchange

FHIR improves administrative efficiency:

- **Prior authorization**: Check coverage and submit auth requests via FHIR
- **Formulary checking**: Real-time medication coverage lookup
- **Claims attachments**: Send supporting documentation with claims
- **Coverage discovery**: Find patient's insurance coverage

### 8. Clinical Decision Support

FHIR enables evidence-based care:

- **CDS Hooks**: Cards appear at the right time (e.g., "Consider diabetes screening")
- **Order sets**: FHIR RequestGroup for protocol-driven ordering
- **Care plans**: FHIR CarePlan for chronic disease management
- **Drug interaction checking**: FHIRcast for real-time prescription review

---

## How FHIR is Used in VPR

### 1. Wire Format for Coordination Data

VPR uses **FHIR-aligned wire formats** for coordination repository metadata:

**Conceptual Alignment, Not Implementation:**

VPR does not implement:

- FHIR REST APIs
- FHIR JSON or XML formats
- FHIR resource validation
- FHIR server capabilities

Instead, VPR uses FHIR **semantics** in YAML wire formats:

**COORDINATION_STATUS.yaml:**

Tracks coordination repository lifecycle:

```yaml
coordination_id: "7f4c2e9d-4b0a-4f3a-9a2c-0e9a6b5d1c88"
clinical_id: "a4f91c6d-3b2e-4c5f-9d7a-1e8b6c0a9f12"
status:
  lifecycle_state: active
  record_open: true
  record_queryable: true
  record_modifiable: true
```

This corresponds conceptually to resource status tracking in FHIR.

**Thread ledger.yaml:**

Messaging thread metadata uses FHIR Communication resource semantics:

```yaml
communication_id: 20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000
status: open # Maps to Communication.status
participants:
  - participant_id: 4f8c2a1d-9e3b-4a7c-8f1e-6b0d-2c5a9f12
    role: clinician # Maps to Communication.recipient
    display_name: Dr Jane Smith
```

Key mappings:

- `communication_id` → `Communication.identifier`
- `status` → `Communication.status` (open=in-progress, closed=completed, archived=stopped)
- `participants` → `Communication.recipient` array
- `created_at` → `Communication.sent`
- `visibility.sensitivity` → `Communication.meta.security`

### 2. FHIR Module in VPR Core

The `fhir` crate provides wire format handling:

**Module: `fhir::CoordinationStatus`**

```rust
// Parse COORDINATION_STATUS.yaml
let status_data = fhir::CoordinationStatus::parse(yaml_text)?;

// Render to YAML
let yaml = fhir::CoordinationStatus::render(&status_data)?;
```

Domain types:

- `CoordinationStatusData` - Top-level structure
- `StatusInfo` - Status details
- `LifecycleState` - Active, Suspended, Closed

**Module: `fhir::Messaging`**

```rust
// Parse thread ledger.yaml
let ledger_data = fhir::Messaging::ledger_parse(yaml_text)?;

// Render to YAML
let yaml = fhir::Messaging::ledger_render(&ledger_data)?;
```

Domain types:

- `LedgerData` - Thread metadata
- `ThreadStatus` - Open, Closed, Archived
- `LedgerParticipant` - Participant with role
- `ParticipantRole` - Clinician, Patient, CareTeam, System

### 3. Semantic Preservation for Future Projections

VPR's FHIR-aligned design enables future conversions:

**FHIR Communication Projection:**

VPR messaging threads can be projected to FHIR Communication resources:

```json
{
  "resourceType": "Communication",
  "id": "20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000",
  "status": "in-progress",
  "sent": "2026-01-11T14:35:22.045Z",
  "recipient": [
    {
      "reference": "Practitioner/4f8c2a1d-9e3b-4a7c-8f1e-6b0d-2c5a9f12",
      "display": "Dr Jane Smith"
    }
  ],
  "payload": [
    {
      "contentString": "Patient has reported increasing shortness of breath..."
    }
  ]
}
```

**FHIR Task Projection:**

Future coordination tasks could map to FHIR Task resources:

- `Task.status` - requested, accepted, in-progress, completed
- `Task.intent` - order, plan, proposal
- `Task.code` - Type of task
- `Task.for` - Patient reference
- `Task.owner` - Responsible practitioner
- `Task.requester` - Who requested the task

**FHIR DocumentReference:**

OpenEHR compositions could be exposed as FHIR DocumentReference:

```json
{
  "resourceType": "DocumentReference",
  "status": "current",
  "type": {
    "coding": [
      {
        "system": "http://loinc.org",
        "code": "34133-9",
        "display": "Summary of episode note"
      }
    ]
  },
  "content": [
    {
      "attachment": {
        "contentType": "application/yaml",
        "url": "/clinical/a4/f9/a4f91c6d.../composition.yaml"
      }
    }
  ]
}
```

### 4. API Gateway Projection

VPR can expose FHIR APIs via an API gateway:

**REST API (future):**

```http
GET /fhir/Communication?subject=Patient/123
GET /fhir/Patient/123
POST /fhir/Communication
PUT /fhir/Communication/456
```

The API gateway would:

1. Receive FHIR REST requests
2. Translate to VPR operations
3. Execute on Git-based repository
4. Project results to FHIR format
5. Return FHIR responses

**GraphQL API (future):**

```graphql
query {
  patient(id: "123") {
    name
    communications {
      sent
      sender
      payload
    }
  }
}
```

### 5. Terminology Binding

VPR uses FHIR's approach to coded data:

**Participant Roles:**

```yaml
role: clinician # Maps to FHIR ParticipantRole value set
```

Future binding to standard terminologies:

- SNOMED CT for clinical concepts
- LOINC for observations and documents
- Local code systems for organization-specific concepts

**Visibility/Sensitivity:**

```yaml
sensitivity: confidential # Maps to FHIR security labels
```

Alignment with:

- `http://terminology.hl7.org/CodeSystem/v3-Confidentiality`
- Values: N (normal), R (restricted), V (very restricted)

### 6. FHIR Bulk Data Export

VPR's Git-based storage supports bulk data patterns:

**Patient-level export:**

```http
GET /fhir/$export?_type=Communication,Observation,Condition
```

Would generate:

- NDJSON files with FHIR resources
- Parallel processing of patient repositories
- Streaming output via polling pattern

**Group-level export:**

```http
GET /fhir/Group/high-risk-patients/$export
```

Cohort definition → Git repository query → FHIR resource generation

### 7. SMART on FHIR Integration

VPR can support SMART app launches:

**Standalone Launch:**

1. App redirects to VPR authorization endpoint
2. User authenticates and authorizes scopes
3. App receives access token
4. App queries VPR FHIR API

**EHR Launch:**

1. EHR launches SMART app with context (patient, encounter)
2. App exchanges launch token for access token
3. App queries VPR for contextual data

**Scopes:**

- `patient/Communication.read` - Read patient's messages
- `patient/Observation.read` - Read patient's observations
- `user/Practitioner.read` - Read clinician's profile
- `launch/patient` - Patient context available

### 8. CDS Hooks Integration

VPR could provide clinical decision support:

### Hook: patient-view

Triggered when clinician opens patient chart:

```json
{
  "hookInstance": "abc123",
  "hook": "patient-view",
  "context": {
    "patientId": "123",
    "userId": "Practitioner/456"
  }
}
```

VPR could return cards suggesting:

- Unread messages in coordination threads
- Overdue care plan activities
- Missing documentation

### 9. FHIR Subscriptions

VPR could support change notifications:

**Subscription creation:**

```json
{
  "resourceType": "Subscription",
  "status": "requested",
  "criteria": "Communication?subject=Patient/123",
  "channel": {
    "type": "rest-hook",
    "endpoint": "https://example.org/webhook",
    "payload": "application/fhir+json"
  }
}
```

Git post-receive hooks could trigger subscription notifications.

### 10. Deviations from Standard FHIR

VPR adapts FHIR concepts for version-controlled storage:

**Storage:**

- Git repositories, not FHIR server databases
- YAML wire formats, not JSON/XML
- File-based, not API-first

**Versioning:**

- Git commits, not FHIR resource versions
- Immutable files, not REST versioning
- Complete history always available

**Search:**

- File system traversal, not FHIR search parameters (yet)
- Git log queries, not database queries
- Future: AQL or FHIR search translation layer

**Transactions:**

- Git atomic commits, not FHIR Bundle transactions
- Repository-level consistency, not resource-level

**Rationale:**

This provides:

- Human-readable audit trails
- Cryptographic signing and verification
- Distributed version control
- No runtime database dependencies
- Standard tooling (Git, text editors)

---

## Future FHIR Integration

VPR's FHIR-aligned design supports progressive enhancement:

### Near-term (Phase 1)

- **REST API gateway**: Expose FHIR resources via HTTP
- **Read-only operations**: GET for Communication, Patient, Practitioner
- **Basic search**: `?subject`, `?date`, `?status` parameters
- **SMART on FHIR**: OAuth 2.0 authorization for app access

### Medium-term (Phase 2)

- **Write operations**: POST, PUT for creating/updating resources
- **Bulk data export**: System-level and patient-level export
- **FHIR Subscriptions**: Webhook notifications for changes
- **Advanced search**: Full FHIR search parameter support

### Long-term (Phase 3)

- **CDS Hooks**: Real-time clinical decision support integration
- **FHIR Questionnaire**: Structured data collection forms
- **GraphQL API**: Flexible querying alternative to REST
- **FHIR Mapping Language**: Automated OpenEHR ↔ FHIR translation

---

## References

- [FHIR Specification](https://hl7.org/fhir/)
- [FHIR Resource List](https://hl7.org/fhir/resourcelist.html)
- [SMART on FHIR](https://smarthealthit.org/)
- [CDS Hooks](https://cds-hooks.org/)
- [FHIR Bulk Data Access](https://hl7.org/fhir/uv/bulkdata/)
- [US Core Implementation Guide](https://www.hl7.org/fhir/us/core/)
- [VPR FHIR Integration](../technical/coordination/fhir.md)
- [VPR Coordination Repository](../technical/coordination/index.md)
