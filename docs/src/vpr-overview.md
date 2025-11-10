# Overview

## Introduction

In today’s healthcare landscape, Electronic Patient Records (EPRs) are traditionally stored in centralised databases that serve the needs of organisations – such as hospitals, GP practices, and social care settings. While this approach is familiar and widely adopted, it can make it harder for patients to directly access, understand, and flag errors in their own health information.

There are areas of improvement in this space – for example, patients in the UK can access parts of their record through the NHS App in the UK ([NHS 2015](#nhs-2025)) or local patient portals. However, these systems are still organisation-owned, fragmented, and often limited in scope.

The Versioned Patient Repository (VPR) introduces a shift in this paradigm by placing patients at the centre of their health and care record.

### Patients first

We put the patient first in every decision – from the underlying architecture, to the APIs, to how information is presented and controlled. The VPR is not merely compatible with a patient-first model – it is built around it. The patient is front and centre, and everything else is designed to support that core.

The patient holds a complete, versioned, and portable record that reflects their health and care journey across settings. Instead of organisations needing to broker complex integrations, the VPR offers a single, patient-held data layer – a consistent source of truth available wherever care is delivered.

Clinicians, health professionals, support staff, and organisations connect to this patient-centred data layer via secure APIs and apps. They do not own or control the data – they interact with it. This rebalances the data model: the patient becomes the anchor, and all systems work around them.

Of course, excellent user-centred design remains critical for all users. Clinical and organisational tools built on the VPR are designed for performance, clarity, and safety. But the underlying record is always shaped first by what best serves the patient.

### Benefits of the VPR design

Placing the patient first unlocks multiple benefits. Wherever a patient can go, their record should follow – and the VPR makes this possible by using openEHR data structures to support interoperability by default.

From a safety perspective, patients can more easily spot errors or inconsistencies, adding an extra layer of assurance. From a financial and operational standpoint, the lightweight, open-source model of the VPR reduces infrastructure burden and supports cost-effective deployment in both small and large settings.

### Technical Details

The VPR uses an open-source, file-based, version-controlled architecture aligned with openEHR data standards. It offers a transparent, portable, and secure method of managing health records that retains the benefits of traditional audit trails – similar to paper notes – but with modern digital flexibility.

Every change becomes a new version: data is never overwritten, and both patients and organisations can explore the full history. Yet the system is simple to use – patients do not need technical expertise. The record can be explored via an app, downloaded for offline use, or transferred between providers without complex tooling.

The VPR is written in Rust, offering a fast, safe, and cross-platform backend that can be compiled differently for patient-facing or organisation-facing use. It supports a range of deployment options – from standalone apps to server-based installations. The codebase is modular and extensible, built as a collection of Rust crates with well-defined APIs.

Compile-time Rust flags are used to include or exclude functionality depending on the build context. For example, patient-side builds include only what is needed to view and manage one's own record, while organisation-side builds can include logic for managing multiple patients, access controls, and clinical workflows. This ensures the patient-facing experience remains lightweight, secure, and easy to distribute.

### Data Structure and Standards

The underlying data format follows openEHR models, stored as markdown and JSON-compatible files. These act like non-relational documents – self-contained, structured, and readable – making them easy to process in a wide range of applications.

This structure recreates the layered design of openEHR – a clear distinction between data content, clinical models, and terminology – without requiring a centralised relational database.

### Versioning and Audit Trail

Every change to the VPR is committed using Git. Nothing is deleted or lost – a full cryptographic audit trail is preserved. If information is mistakenly entered into the wrong patient's record, it is removed from view, encrypted, and stored securely in a restricted central audit layer. A non-human-readable hash remains in the original record, maintaining traceability without exposing sensitive data.

This guarantees auditability, safety, and trust – even in the face of human error.

### Export and Portability

Patients can download their patient record as a bundle of files - on a USB stick, as an encrypted archive, or even loaded into a standalone reader app. These files remain functional offline and can be interpreted by lightweight applications without needing a local database engine. This simplicity ensures the records remain portable, long-lived, and system-agnostic.

### Natural Progression

The VPR is the natural progression of the patient record, starting with the work of Dr Lawrence Weed in the 1960s.

> There are residents and staff-men who maintain that the content of their records is their own business. In reality, however, it is the patient's business and the business of those who, in the future, will have to depend on that record for the patient's care, or for medical research ([Weed 1964](#weed-1964)).

Lawrence Weed’s Problem-Oriented Medical Record (POMR) reframed medical documentation around the patient’s problems, rather than the clinician’s specialty or the hospital’s structure. His approach established a patient-centred logic for clinical reasoning, in which each problem linked observations, assessments, and plans in a transparent and auditable way.

Building on that foundation, openEHR formalised Weed’s ideas into a computable data model. Its archetypes and templates capture the clinician’s reasoning processes and the structure of clinical encounters, allowing problem-oriented documentation to be represented in interoperable, machine-readable form.

The Versioned Patient Repository (VPR) extends these principles further. The VPR provides a longitudinal, versioned record that preserves data integrity across institutions and regions, giving both patients and clinicians access to a single evolving source of truth. Where Weed’s POMR unified thought, and openEHR unified meaning, the VPR unifies time and ownership.

---

## References

<span id="nhs-2025"></span>  
NHS (2025). 'Personal health records'. Available at: [https://www.nhs.uk/nhs-app/nhs-app-help-and-support/health-records-in-the-nhs-app/personal-health-records/](https://www.nhs.uk/nhs-app/nhs-app-help-and-support/health-records-in-the-nhs-app/personal-health-records/) (Accessed: 5 Nov. 2025).

<span id="weed-1964"></span>
WEED, L.L. (1964). 'MEDICAL RECORDS, PATIENT CARE, AND MEDICAL EDUCATION', *It. J. Med. Sc.*, 462, pp. 271-82.
