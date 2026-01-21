# Healthcare Standards

VPR is built on two foundational healthcare standards: **OpenEHR** and **FHIR**. These standards provide complementary capabilities for clinical data management and interoperability.

## Overview

### OpenEHR

OpenEHR provides a vendor-independent architecture for **storing and managing** clinical data with built-in versioning, semantic interoperability, and clinical knowledge separation.

VPR uses OpenEHR for:

- Clinical record structure and composition model
- EHR status tracking and identity linkage
- Version-controlled clinical data management
- Archetype-based semantic definitions

[Read more about OpenEHR in VPR →](openehr.md)

### FHIR

Fast Healthcare Interoperability Resources (FHIR) is a modern standard for **exchanging** healthcare data via RESTful APIs, with emphasis on ease of implementation and web-friendly formats.

VPR uses FHIR for:

- Coordination repository wire formats
- Messaging thread semantics (Communication resource)
- Future API projections and integrations
- Interoperability with external systems

[Read more about FHIR in VPR →](fhir.md)

---

## Complementary Roles

OpenEHR and FHIR serve different but complementary purposes in VPR:

| Aspect                | OpenEHR                         | FHIR                              |
| --------------------- | ------------------------------- | --------------------------------- |
| **Primary focus**     | Long-term clinical data storage | Real-time data exchange           |
| **Architecture**      | Repository-based, versioned     | API-first, resource-based         |
| **Granularity**       | Document-level (Compositions)   | Element-level (Resources)         |
| **Versioning**        | Built-in, audit-focused         | Optional, implementation-specific |
| **Clinical modeling** | Archetypes + Templates          | Profiles + Implementation Guides  |
| **Best for**          | EHR systems, clinical archives  | HIE, mobile apps, integrations    |

### VPR's Hybrid Approach

VPR combines the strengths of both standards:

**OpenEHR for Clinical Records:**

- Compositions stored in clinical repository
- Full version history via Git
- Archetype-based semantic structure
- Long-term clinical archive

**FHIR for Coordination:**

- Communication semantics for messaging
- RESTful API patterns for future integration
- Resource-based wire formats
- Interoperability with external systems

This hybrid approach provides:

- **Best-in-class storage**: OpenEHR's robust clinical data model
- **Best-in-class exchange**: FHIR's practical API standards
- **Future flexibility**: Can project either standard externally
- **Standards alignment**: Both use standard terminologies (SNOMED, LOINC)

---

## Design Principles

### Semantic Preservation

VPR maintains the **meaning** of both standards:

- OpenEHR composition structure is preserved
- FHIR resource semantics are followed
- Mappings between standards are explicit
- No information loss in either direction

### Implementation Pragmatism

VPR adapts standards for version-controlled storage:

- YAML instead of JSON/XML for human readability
- Git instead of database for version control
- File-based storage for simplicity and auditability
- Cryptographic signing for integrity

### Progressive Enhancement

VPR can add standard APIs incrementally:

- Core storage model is standards-aligned
- APIs can be added without changing storage
- Multiple projections possible (OpenEHR API, FHIR API, GraphQL)
- Storage remains authoritative source

---

## Standards Governance

### OpenEHR Foundation

- Develops and maintains OpenEHR specifications
- Curates archetype repositories (Clinical Knowledge Manager)
- Provides conformance testing
- International community of users

**VPR Compliance:**

- Uses OpenEHR Reference Model structures
- Declares RM version in all files
- Follows composition and versioning semantics
- Compatible with OpenEHR tooling (parsers, validators)

### HL7 International

- Develops and maintains FHIR specifications
- Manages terminology and code systems
- Provides implementation guides and profiles
- Large ecosystem of vendors and implementers

**VPR Compliance:**

- Uses FHIR resource semantics (conceptual alignment)
- Wire formats map to FHIR resources
- Can project to FHIR REST API
- Compatible with FHIR tooling (validators, servers)

---

## Further Reading

- [OpenEHR in VPR](openehr.md) - Detailed coverage of OpenEHR usage
- [FHIR in VPR](fhir.md) - Detailed coverage of FHIR integration
- [Clinical Repository Design](../technical/clinical/index.md)
- [Coordination Repository Design](../technical/coordination/index.md)
- [Technical Architecture](../technical/index.md)

---

## External Resources

### OpenEHR

- [OpenEHR Specification](https://specifications.openehr.org/)
- [Clinical Knowledge Manager](https://ckm.openehr.org/)
- [OpenEHR Foundation](https://www.openehr.org/)

### FHIR

- [FHIR Specification](https://hl7.org/fhir/)
- [FHIR Resource List](https://hl7.org/fhir/resourcelist.html)
- [SMART on FHIR](https://smarthealthit.org/)
- [Implementation Guides](https://www.fhir.org/guides/registry/)
