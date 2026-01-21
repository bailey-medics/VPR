# OpenEHR Standard

## Overview

OpenEHR is an open standard specification for electronic health records (EHR) that provides a vendor-independent, future-proof architecture for storing and managing clinical data. Developed by the OpenEHR Foundation, it separates clinical knowledge (archetypes) from technical implementation, enabling healthcare systems to evolve without requiring system rewrites.

### Core Concepts

**Reference Model (RM):**

The Reference Model defines the stable, information structures for representing EHR data. It includes:

- **Compositions**: Documents or clinical encounters (e.g., discharge summaries, lab reports)
- **Entries**: Individual clinical statements (observations, evaluations, instructions, actions)
- **Data structures**: Elements, items, clusters for organizing clinical data
- **Version control**: Built-in versioning for all clinical data

**Archetypes:**

Archetypes are reusable, computable definitions of clinical concepts (e.g., "blood pressure", "medication order"). They:

- Define the structure and constraints for specific clinical concepts
- Are vendor-neutral and language-independent
- Can be shared across systems and jurisdictions
- Are maintained in centralized repositories (Clinical Knowledge Manager)

**Templates:**

Templates combine multiple archetypes into specific clinical documents (e.g., "Emergency Department Admission", "Diabetes Review"). They:

- Constrain archetypes further for specific use cases
- Define which archetypes are mandatory or optional
- Specify terminology bindings
- Configure the data collection interface

**Terminology Integration:**

OpenEHR supports binding to standard terminologies:

- SNOMED CT (clinical terms)
- LOINC (laboratory and clinical observations)
- ICD-10/ICD-11 (diagnoses)
- Local terminologies as needed

---

## Problems OpenEHR Solves

### 1. Semantic Interoperability

**Problem:** Different EHR systems represent the same clinical concept in incompatible ways, making data exchange difficult and error-prone.

**Solution:** Archetypes provide standardized, computable definitions of clinical concepts that work across systems. A "blood pressure" archetype means the same thing regardless of vendor.

### 2. Vendor Lock-in

**Problem:** Healthcare organizations become dependent on proprietary EHR systems, making migration expensive and risky.

**Solution:** OpenEHR's vendor-neutral data model allows data to be stored in a portable format. Organizations can switch systems without data conversion.

### 3. Clinical Knowledge Evolution

**Problem:** Medical knowledge evolves faster than software development cycles. Adding new clinical concepts requires expensive system updates.

**Solution:** Archetypes can be created, modified, and deployed independently of the underlying software. Clinicians and informaticians can define new concepts without programmer intervention.

### 4. Data Quality and Validation

**Problem:** EHR systems often allow inconsistent or invalid data entry, compromising clinical safety.

**Solution:** Archetypes define constraints and validation rules at the clinical knowledge level, ensuring data quality at the point of entry.

### 5. Longitudinal Health Records

**Problem:** Patient data is fragmented across multiple systems, time periods, and care settings.

**Solution:** OpenEHR's version-controlled composition model maintains complete audit trails and supports lifelong health records across organizational boundaries.

### 6. Research and Analytics

**Problem:** Clinical data locked in proprietary formats is difficult to query for research and quality improvement.

**Solution:** OpenEHR's structured, semantically-defined data supports sophisticated querying (via AQL - Archetype Query Language) and data extraction.

---

## How OpenEHR is Normally Used in Digital Health

### 1. National EHR Programs

OpenEHR is used for national-scale EHR deployments:

- **Norway**: National EHR platform (Helse Vest)
- **Slovenia**: National EHR infrastructure
- **Brazil**: Public health information systems
- **Russia**: National digital health initiatives

These implementations provide unified clinical data repositories serving entire populations.

### 2. Hospital Information Systems

OpenEHR-based clinical data repositories (CDRs) serve as:

- Central clinical data stores for hospital groups
- Integration hubs connecting departmental systems
- Long-term clinical archives replacing legacy systems

### 3. Clinical Decision Support

OpenEHR's structured data enables:

- Rules-based clinical decision support
- Guideline execution engines
- Drug interaction checking
- Clinical pathways automation

### 4. Research Data Platforms

OpenEHR supports:

- Cohort identification for clinical trials
- Observational research databases
- Quality improvement analytics
- Population health monitoring

### 5. Citizen Health Records

OpenEHR powers patient portals and personal health records:

- Patient-accessible health data
- Patient-entered observations (blood pressure, glucose)
- Shared decision-making tools
- Care plan tracking

### 6. Specialized Clinical Systems

OpenEHR is used in domain-specific applications:

- Intensive care monitoring systems
- Oncology treatment records
- Maternal and child health tracking
- Chronic disease management

---

## How OpenEHR is Used in VPR

### 1. Clinical Record Structure

VPR uses OpenEHR Reference Model structures for clinical compositions:

**EHR Status:**

Every patient has an `ehr_status.yaml` file following OpenEHR's EHR_STATUS specification:

```yaml
_type: EHR_STATUS
subject:
  _type: PARTY_SELF
is_queryable: true
is_modifiable: true
uid:
  _type: HIER_OBJECT_ID
  value: "a4f91c6d-3b2e-4c5f-9d7a-1e8b6c0a9f12"
```

This provides:

- Patient identity linkage
- Record queryability flags
- Modification permissions
- External references to demographics

**Compositions:**

Clinical documents (letters, observations) use OpenEHR COMPOSITION structure:

- `_type: COMPOSITION` declares the document type
- `name` provides human-readable document title
- `archetype_node_id` identifies the template/archetype used
- `uid` provides version-controlled unique identifier
- `context` captures care setting metadata
- `content` contains the clinical data entries

**Example composition.yaml for a clinical letter:**

```yaml
_type: COMPOSITION
name:
  _type: DV_TEXT
  value: Clinical Letter
archetype_node_id: openEHR-EHR-COMPOSITION.correspondence.v0
uid:
  _type: HIER_OBJECT_ID
  value: "20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000"
language:
  _type: CODE_PHRASE
  terminology_id:
    _type: TERMINOLOGY_ID
    value: ISO_639-1
  code_string: en
```

### 2. Version-Controlled Repository Model

VPR adopts OpenEHR's versioning philosophy:

**Immutability:**

- Clinical compositions are immutable once committed
- Changes create new versions with full audit trail
- Git provides the versioning infrastructure
- Every composition has a unique timestamp-prefixed ID

**Contribution Model:**

Each Git commit represents an OpenEHR CONTRIBUTION:

- Contains one or more VERSION<COMPOSITION> objects
- Records who made the change (commit author)
- Records when the change occurred (commit timestamp)
- Records why the change was made (commit message)

### 3. Semantic Interoperability

VPR uses OpenEHR conventions for:

**Reference Model Version:**

All files declare their RM version for compatibility:

```yaml
_rm_version: "1.1.0"
```

This ensures:

- Parsers know which specification to apply
- Forward/backward compatibility can be managed
- Systems can validate against the correct schema

**Type Annotations:**

Every complex object declares its `_type` for unambiguous parsing:

- `_type: COMPOSITION`
- `_type: DV_TEXT`
- `_type: DV_CODED_TEXT`
- `_type: PARTY_SELF`

### 4. Clinical Data Query Support

VPR's structured data enables OpenEHR-style querying:

**Archetype paths:**

Data elements are addressable via standardized paths:

```
/content[openEHR-EHR-OBSERVATION.blood_pressure.v2]/data/events[at0006]/data/items[at0004]/value
```

This allows:

- Precise data extraction
- Cross-system queries
- Research cohort identification
- Quality improvement analytics

### 5. Template-Based Data Collection

VPR uses OpenEHR templates for:

**Clinical Document Templates:**

Templates stored in `crates/core/templates/clinical/` define:

- Which archetypes are included
- Mandatory vs. optional elements
- Terminology bindings
- Default values and constraints

**Initialization from Templates:**

When creating a new clinical record, VPR:

1. Validates the template directory exists
2. Copies template files to the patient's repository
3. Initializes `ehr_status.yaml` with proper structure
4. Commits the initial state to Git

### 6. Deviations from Standard OpenEHR

VPR adapts OpenEHR for a version-controlled repository model:

**Storage Format:**

- Uses YAML instead of JSON or XML for human readability
- One composition per file for Git-friendly diffs
- Markdown for narrative content (e.g., letter body)

**Server Architecture:**

- No OpenEHR REST API server
- No query engine (yet)
- File-based storage instead of database
- Git instead of versioning database

**Rationale:**

This provides:

- Human-readable audit trails
- Standard version control tooling
- Cryptographic signing and verification
- Distribution and replication via Git
- No runtime database dependencies

### 7. Future OpenEHR Integration

VPR is designed to support future OpenEHR capabilities:

**Archetype Query Language (AQL):**

The structured data format will support AQL queries:

```sql
SELECT
  c/uid/value,
  c/context/start_time,
  o/data[at0001]/events[at0006]/data[at0003]/items[at0004]/value
FROM
  EHR e
  CONTAINS COMPOSITION c
  CONTAINS OBSERVATION o[openEHR-EHR-OBSERVATION.blood_pressure.v2]
WHERE
  o/data[at0001]/events[at0006]/data[at0003]/items[at0004]/value/magnitude > 140
```

**API Projections:**

VPR compositions can be projected to:

- OpenEHR REST API responses
- RM-compliant JSON
- Canonical XML format
- FHIR resources (via mappings)

**Template Server:**

Future template management:

- Operational Template (OPT) import
- Template validation
- Web-based template designer integration
- Archetype repository synchronization

---

## References

- [OpenEHR Specification](https://specifications.openehr.org/)
- [Clinical Knowledge Manager](https://ckm.openehr.org/)
- [OpenEHR Foundation](https://www.openehr.org/)
- [Archetype Query Language (AQL)](https://specifications.openehr.org/releases/QUERY/latest/AQL.html)
- [VPR Clinical Repository Design](../technical/clinical/index.md)
