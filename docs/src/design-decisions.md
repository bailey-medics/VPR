# Design decisions

## Separation of Patient Demographics and Clinical Data

In VPR, patient demographics and clinical data are stored separately to preserve privacy, improve data integrity, and follow openEHR principles. The demographics repository (equivalent to a Master Patient Index) contains personal identifiers such as name, date of birth, and national ID, while the clinical repository holds all medical content including observations, encounters, and results. The only connection between them is a reference stored in the clinical repository’s ehr_status.subject.external_ref, which points to the corresponding demographic record. This separation allows clinical data to be shared, versioned, and audited independently of personally identifiable information, supporting both patient confidentiality and modular system design.

Ref: [https://specifications.openehr.org/releases/1.0.1/html/architecture/overview/Output/design_of_ehr.html](https://specifications.openehr.org/releases/1.0.1/html/architecture/overview/Output/design_of_ehr.html)
