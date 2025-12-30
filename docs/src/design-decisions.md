# Design decisions

## Separation of Patient Demographics and Clinical Data

In VPR, patient demographics and clinical data are stored separately to preserve privacy, improve data integrity, and follow openEHR principles. The demographics repository (equivalent to a Master Patient Index) contains personal identifiers such as name, date of birth, and national ID, while the clinical repository holds all medical content including observations, encounters, and results. The only connection between them is a reference stored in the clinical repository’s ehr_status.subject.external_ref, which points to the corresponding demographic record. This separation allows clinical data to be shared, versioned, and audited independently of personally identifiable information, supporting both patient confidentiality and modular system design.

Ref: [https://specifications.openehr.org/releases/1.0.1/html/architecture/overview/Output/design_of_ehr.html](https://specifications.openehr.org/releases/1.0.1/html/architecture/overview/Output/design_of_ehr.html)

FHIR for demographics

openEHR for clinical data

## Shard directory structure

VPR uses sharded directories to keep filesystem performance predictable and scalable as the number of patient or EHR repositories grows. Instead of placing every record under a single massive directory—which can slow down lookups and directory listings on most filesystems—VPR distributes repositories across multiple subfolders based on a hash or prefix of their UUID. This structure ensures faster access times, avoids inode exhaustion, and keeps Git operations efficient even with thousands of records. In short, sharding prevents filesystem bottlenecks by spreading data evenly across smaller, manageable directory trees.

## Testing

When testing VPR’s file creation and repository logic, it is best to use real temporary directories rather than mocked filesystems. Because file creation and structure are central to VPR’s design, tests should verify that directories, Git repositories, and configuration files are created exactly as they will be in production. Using crates such as tempfile or assert_fs allows tests to write into isolated, automatically cleaned-up folders, ensuring realism without cluttering the developer’s system. This approach validates not only path logic but also permissions, file naming, and serialisation behaviour—details that mocks often overlook. In short, VPR’s tests should interact with the filesystem genuinely but safely, using temporary sandboxes to confirm that the system builds and cleans up its data as intended.

VPR uses the tempfile crate for testing file creation and repository logic. The crate provides automatically managed temporary files and directories that are securely created in the system’s temp folder and deleted when the test ends. Because VPR’s core functionality involves creating and managing filesystem structures, these tests must interact with real files rather than mocks. Using tempfile::TempDir allows us to test against the actual operating system, validating path resolution, permissions, serialisation, and cleanup behaviour without leaving residual data on the developer’s machine. This ensures our tests remain both realistic and self-contained, accurately reflecting production behaviour while maintaining a clean and reproducible testing environment.

## Signed Commits in VPR

VPR requires all Git commits to be cryptographically signed to guarantee the authenticity and integrity of every change made to a patient’s digital record. Each commit forms part of the clinical audit trail, ensuring that the author of every modification—whether a clinician, developer, or automated process—can be verified beyond doubt. This provides non-repudiation (proof that a specific individual authorised a change) and makes any tampering or unauthorised alteration immediately detectable. In healthcare systems, where patient safety and data provenance are paramount, this level of assurance is essential.

To achieve this, VPR mandates the use of X.509 certificates for commit signing. X.509 is the same internationally recognised standard that underpins secure web traffic (TLS), encrypted email (S/MIME), and enterprise PKI systems. Each certificate binds a cryptographic key to a verified organisational identity and includes expiry and revocation capabilities. This enables hospitals or healthcare organisations to centrally issue, manage, and revoke clinician or system signing certificates as part of their normal governance and information security processes.

Alternative signing methods such as SSH or GPG were intentionally not adopted. SSH keys, while simple, lack built-in identity validation, expiry, or revocation mechanisms, making them unsuitable for regulated environments. GPG, though capable of strong cryptography, relies on a decentralised “web of trust” model that does not align with the controlled identity assurance required in clinical governance. In contrast, X.509 certificates provide a trusted, hierarchical chain of identity that integrates directly with organisational PKI and complies with established healthcare and security standards.

In short, VPR’s mandatory use of X.509-signed commits transforms every repository into a tamper-evident, cryptographically verifiable clinical ledger, ensuring that each change is traceable to an authenticated, accountable individual within a trusted institutional framework.

### The core idea

In the NHS, X.509 certificates are primarily used for identity, authentication, and trust between systems. They underpin who you are and whether a system should trust you, not what you have written clinically.

The anchor for all of this is the NHS Public Key Infrastructure (PKI), operated nationally under NHS England.

#### 1. NHS Smartcards – the most visible use of X.509

NHS smartcards are the clearest, most widespread use of X.509 in day-to-day clinical life.

Each smartcard contains X.509 digital certificates issued by the NHS PKI. These certificates are:

- Bound to a real person (for example, a clinician)
- Linked to their role and organisation
- Protected by the physical card and a personal identification number

When a clinician inserts their smartcard and enters their personal identification number:

- The certificate on the card is presented to the system
- The system validates the certificate against the NHS PKI trust chain
- The clinician is authenticated as a known, trusted individual

This is certificate-based authentication, not just username and password.

Crucially, the certificate is used to log in, not to sign individual clinical entries.

#### 2. Role-Based Access Control (RBAC)

Once authenticated via the smartcard certificate, NHS systems then apply role-based access control.

The certificate proves identity.
The role assignment (for example, consultant, registrar, pharmacist) determines what the user can do.

X.509 plays no direct role here beyond establishing a trusted identity at login. The permissions logic lives in directories and access control services, not in the certificate itself.

#### 3. Access to national NHS systems (Spine services)

Smartcard-based X.509 authentication is required for access to many national services, including:

- Patient demographic services
- Summary Care Records
- Prescription and dispensing systems
- Other Spine-connected applications

These systems trust users because they trust the NHS PKI certificate chain, not because they trust the local hospital.

This gives the NHS a nationally consistent trust model.

#### 4. System-to-system communication (mutual transport layer security)

X.509 is also widely used between systems, not just for people.

NHS services often use mutual transport layer security, where:

- Both client and server present X.509 certificates
- Each side validates the other against trusted certificate authorities

This is common for:

- Spine integrations
- Messaging between organisations
- Secure application programming interfaces

Again, this secures the connection, not the content.

#### 5. Electronic signatures and formal approvals

X.509 underpins electronic signature mechanisms used for:

- Legal documents
- Contracts
- Regulatory submissions
- Some consent workflows

These signatures apply to documents, not to the live patient record data model.

They are usually implemented as separate workflows layered on top of clinical systems.

## The patient's voice

VPR stores both professional clinical entries and patient contributions in the same repository, using distinct artefact paths.
Clinical truth is authored and signed under /clinical/.
Patient input is preserved under /patient/ and may inform, but never overwrite, clinical records without professional review.

## A single branch repository

We only use a single "refs/heads/main" branch per repository in VPR. However, this is only policy, as Git itself allows multiple branches.
