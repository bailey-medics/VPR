# Overview

## Introduction

In today’s healthcare landscape, Electronic Patient Records (EPRs) are traditionally stored in centralised databases that serve the needs of organisations – such as hospitals, GP practices, and social care settings. While this approach is familiar and widely adopted, it can make it harder for patients to own their own data, directly access their data, understand it, and flag errors.

There are areas of improvement in this space – for example, patients in the UK can access parts of their record through the NHS App in the UK ([NHS 2015](#nhs-2025)) or local patient portals. However, these systems are still organisation-owned, lacking interoperability, fragmented, and often limited in scope.

The Versioned Patient Repository (VPR) introduces a shift in this paradigm by placing patients at the centre of their health and care record.

The VPR is a file-based health record architecture where each patient’s data is stored as structured, human-readable documents. Instead of overwriting records, each change creates a new version, managed through Git-like version control. This produces an immutable audit trail while maintaining portability and interoperability.

At the heart of VPR is a combined keystone principle: the patient comes first, and the canonical record is kept as human-readable files. Every design choice should reinforce patient agency while preserving an auditable, legible, file-based record that patients and clinicians alike can inspect and carry with them.

### Patients first

When treating a patient, we put the patient at the heart of every decision. Their needs, preferences, and rights guide our actions. The same patient-first principle should extend to health data. Current EPR implementations, however, are built around organisational needs rather than those of the individual. We need to step back and reimagine the health record from the patient’s perspective. In fact, we need to make the patient's data portable and accessible wherever they go. This is where the `file` shows its strength.

The VPR is a file-based data storage structure. To ensure data integrity and traceability, data entered into the VPR record is stored and signed off via the use of version control. Git is used as the underlying technology to manage versioning along with cryptography.

Using VPR, the patient holds a complete, versioned, and portable record that reflects their health and care journey across settings. Instead of organisations needing to broker complex integrations, the VPR offers a single, patient-held data layer – a consistent source of truth available wherever care is delivered.

### Benefits of the VPR design

Placing the patient first unlocks multiple benefits. Wherever a patient can go, their record should follow – and the VPR makes this possible by using standard data structures to support interoperability by default.

From a safety perspective, patients can more easily spot errors or inconsistencies, adding an extra layer of assurance. From a financial and operational standpoint, the lightweight, open-source model of the VPR reduces infrastructure burden and supports cost-effective deployment in both small and large settings.

### Technical Details

The Versioned Patient Repository (VPR) is built using a modular, open-source architecture that combines the reliability of file-based storage with the assurance of cryptographic version control. Each patient’s record consists of structured files that are stored and tracked in a Git-based system, ensuring traceability and data integrity. Instead of overwriting data, each change creates a new version that can be reviewed, audited, or rolled back if required.

The VPR is written in Rust, a systems programming language known for its safety, speed, and memory efficiency. The codebase is organised as a collection of independent Rust crates with clearly defined interfaces. This modular approach allows developers to adapt, extend, or replace components without altering the overall structure. For instance, separate crates can handle data storage, versioning logic, cryptographic signing, and API delivery.

The system supports multiple build configurations, enabling the same core codebase to serve both patient-facing and organisation-facing use cases. Compile-time flags determine which functionality is included in each build. A patient-side build contains only the features needed to view and manage one’s own record, keeping it lightweight and secure. An organisation-side build may include additional modules for managing multiple patients, enforcing access controls, and supporting integration with other clinical systems. This approach ensures that both variants remain aligned to the same specification while optimising performance for their respective roles.

Deployment is flexible. The VPR can be embedded within standalone desktop or mobile applications, distributed as an encrypted patient-held package, or hosted on secure institutional servers. Because data is file-based rather than database-bound, deployment does not rely on heavy infrastructure or proprietary database engines. Each record remains portable and can be reconstructed on any compatible instance of the system.

While files act as the canonical data source, efficient access for clinicians and applications requires high-performance querying. To support this, the VPR introduces database-based projections: pre-computed views of the file data that are optimised for specific operations such as patient summaries, correspondence lists, or message threads. Projections can be refreshed automatically whenever a new commit is made, or generated on demand for less frequently accessed data. This design provides the responsiveness of a traditional database while retaining the transparency and auditability of file storage.

Security is embedded at every layer. All files are cryptographically signed and checksummed to prevent tampering. Access is controlled through authenticated APIs, and sensitive data can be encrypted both at rest and in transit. The combination of version control, immutable history, and cryptographic verification ensures that every change is attributable and recoverable, which is essential for clinical safety and regulatory compliance.

In summary, the VPR merges the rigour of modern software engineering with the principles of safe clinical record-keeping. By treating files as the canonical source and databases as transient projections, it achieves both transparency and speed. The result is a system that is secure, flexible, and designed to evolve alongside the healthcare organisations and patients it serves.

> Files as canonical, projections for performance, patient as the atomic unit.

### Data Structure and Standards

The underlying data format follows openEHR models for clinical content and FHIR standards for demographics and coordination data. Files are stored as markdown and JSON-compatible structures. These act like non-relational documents – self-contained, structured, and readable – making them easy to process in a wide range of applications.

Patient data is organised into three separate repositories:

- **Clinical repository**: openEHR-based clinical content (observations, diagnoses, clinical letters)
- **Demographics repository**: FHIR-based patient demographics (name, date of birth, identifiers)
- **Coordination repository** (Care Coordination Repository): care coordination data (encounters, appointments, episodes, referrals) – format to be determined, may adopt FHIR ideologies

This structure recreates the layered design of openEHR – a clear distinction between data content, clinical models, and terminology – while adding administrative coordination as a separate concern. None of these require a centralised relational database.

### Versioning and Audit Trail

Every change to the VPR is committed using Git. Nothing is deleted or lost – a full cryptographic audit trail is preserved. If information is mistakenly entered into the wrong patient's record, it is removed from view, encrypted, and stored securely in a restricted central audit layer. A non-human-readable hash remains in the original record, maintaining traceability without exposing sensitive data.

This guarantees auditability, safety, and trust – even in the face of human error.

### Export and Portability

Patients can download their patient record as a bundle of files - on a USB stick, as an encrypted archive, or even loaded into a standalone reader app. These files remain functional offline and can be interpreted by lightweight applications without needing a local database engine. This simplicity ensures the records remain portable, long-lived, and system-agnostic.

### Natural Progression

The VPR is the natural progression of the patient record, starting with the work of Dr Lawrence Weed in the 1960s.

> There are residents and staff-men who maintain that the content of their records is their own business. In reality, however, it is the patient's business and the business of those who, in the future, will have to depend on that record for the patient's care, or for medical research ([Weed 1964](#weed-1964)).

Lawrence Weed’s Problem-Oriented Medical Record (POMR) reframed medical documentation around the patient’s problems, rather than the clinician’s specialty or the hospital’s structure. His approach established a patient-centred logic for clinical reasoning, in which each problem linked observations, assessments, and plans in a transparent and auditable way.

Building on Dr Weed's foundation, openEHR formalised Weed’s ideas into a computable data model. Its archetypes and templates capture the clinician’s reasoning processes and the structure of clinical encounters, allowing problem-oriented documentation to be represented in interoperable, machine-readable form.

The VPR extends the above principles further. The VPR provides a longitudinal, versioned record that preserves data integrity across institutions and regions, giving both patients and clinicians access to a single evolving source of truth. Where Weed’s POMR unified thought, and openEHR unified meaning, the VPR unifies time and ownership.

---

## References

<span id="nhs-2025"></span>  
NHS (2025). 'Personal health records'. Available at: [https://www.nhs.uk/nhs-app/nhs-app-help-and-support/health-records-in-the-nhs-app/personal-health-records/](https://www.nhs.uk/nhs-app/nhs-app-help-and-support/health-records-in-the-nhs-app/personal-health-records/) (Accessed: 5 Nov. 2025).

<span id="weed-1964"></span>
WEED, L.L. (1964). 'MEDICAL RECORDS, PATIENT CARE, AND MEDICAL EDUCATION', *It. J. Med. Sc.*, 462, pp. 271-82.
