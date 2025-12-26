# Design decisions

## Separation of Patient Demographics and Clinical Data

In VPR, patient demographics and clinical data are stored separately to preserve privacy, improve data integrity, and follow openEHR principles. The demographics repository (equivalent to a Master Patient Index) contains personal identifiers such as name, date of birth, and national ID, while the clinical repository holds all medical content including observations, encounters, and results. The only connection between them is a reference stored in the clinical repository’s ehr_status.subject.external_ref, which points to the corresponding demographic record. This separation allows clinical data to be shared, versioned, and audited independently of personally identifiable information, supporting both patient confidentiality and modular system design.

Ref: [https://specifications.openehr.org/releases/1.0.1/html/architecture/overview/Output/design_of_ehr.html](https://specifications.openehr.org/releases/1.0.1/html/architecture/overview/Output/design_of_ehr.html)

FHIR for demographics

openEHR for clinical data

## Testing

When testing VPR’s file creation and repository logic, it is best to use real temporary directories rather than mocked filesystems. Because file creation and structure are central to VPR’s design, tests should verify that directories, Git repositories, and configuration files are created exactly as they will be in production. Using crates such as tempfile or assert_fs allows tests to write into isolated, automatically cleaned-up folders, ensuring realism without cluttering the developer’s system. This approach validates not only path logic but also permissions, file naming, and serialisation behaviour—details that mocks often overlook. In short, VPR’s tests should interact with the filesystem genuinely but safely, using temporary sandboxes to confirm that the system builds and cleans up its data as intended.

VPR uses the tempfile crate for testing file creation and repository logic. The crate provides automatically managed temporary files and directories that are securely created in the system’s temp folder and deleted when the test ends. Because VPR’s core functionality involves creating and managing filesystem structures, these tests must interact with real files rather than mocks. Using tempfile::TempDir allows us to test against the actual operating system, validating path resolution, permissions, serialisation, and cleanup behaviour without leaving residual data on the developer’s machine. This ensures our tests remain both realistic and self-contained, accurately reflecting production behaviour while maintaining a clean and reproducible testing environment.
