# Design decisions

This document captures the key architectural and governance decisions behind VPR, and the reasoning for each. The emphasis throughout is on auditability, clinical accountability, privacy, and long-term robustness.

---

## Separation of demographics and clinical data

VPR stores **patient demographics** and **clinical data** in separate repositories.

- The **demographics repository** (equivalent to a Master Patient Index) contains personal identifiers such as name, date of birth, and national identifiers.
- The **clinical repository** contains all medical content, including observations, diagnoses, encounters, and results.

The only link between the two is a reference stored in `ehr_status.subject.external_ref` within the clinical repository, pointing to the corresponding demographic record.

This design follows established openEHR principles and provides several benefits:

- Clinical data can be shared, versioned, and audited independently of personally identifiable information.
- Privacy risks are reduced by minimising the spread of identifiers.
- Systems remain modular, allowing demographics and clinical services to evolve separately.

In practice:

- **FHIR** is used for demographics.
- **openEHR** is used for structured clinical data.

Reference:  
https://specifications.openehr.org/releases/1.0.1/html/architecture/overview/Output/design_of_ehr.html

---

## Sharded directory structure

VPR uses **sharded directory layouts** to maintain predictable filesystem performance as the number of patient repositories grows.

Rather than placing all repositories in a single directory, repositories are distributed across subdirectories derived from a UUID prefix or hash. This avoids filesystem bottlenecks, improves lookup performance, and keeps Git operations efficient at scale.

Sharding ensures that the system remains performant and manageable even with very large numbers of patient records.

---

## Testing strategy

VPR’s core functionality depends on real filesystem behaviour. As a result, tests are designed to interact with **actual temporary directories**, not mocked filesystems.

Using crates such as `tempfile`, tests create isolated, automatically cleaned-up directories that closely mirror production behaviour. This allows tests to validate:

- directory creation and layout,
- Git repository initialisation,
- file permissions and naming,
- serialisation and cleanup behaviour.

This approach keeps tests realistic while remaining safe, reproducible, and free from side effects on the developer’s machine.

---

## Signed Git commits in VPR (summary)

VPR uses **cryptographically signed Git commits** to provide immutable, auditable authorship of clinical records.

For signed commits, VPR embeds a **self-contained cryptographic payload directly in the commit object**, not as files in the repository. This payload includes:

- an ECDSA P-256 signature over the canonical commit content,
- the author’s public signing key,
- an optional X.509 certificate issued by a trusted authority (for example a professional regulator).

The private key is generated and held by the author and is never shared or stored in the repository.

Because all verification material is attached to the commit itself, signed VPR commits can be verified **offline**, years later, without reliance on external services. Each commit therefore acts as a sealed attestation linking the clinical change to a named, accountable professional identity.

---

## Why X.509 certificates

VPR mandates the use of **X.509 certificates** for commit signing.

X.509 is the same widely adopted standard used for:

- secure web traffic (Transport Layer Security),
- encrypted email,
- enterprise public key infrastructure,
- regulated identity systems.

Each certificate binds a public key to a verified identity and supports expiry and revocation, making it suitable for regulated healthcare environments.

Other signing mechanisms were deliberately rejected:

- **SSH keys** lack identity assurance, expiry, and revocation.
- **GPG** relies on a decentralised web-of-trust model that does not align with formal clinical governance.

X.509 provides a hierarchical, auditable trust model that fits naturally with healthcare regulation and organisational identity management.

---

## X.509 in the NHS (context)

In the NHS, X.509 certificates are primarily used for **identity and authentication**, not for signing individual clinical entries.

The trust anchor is the **NHS Public Key Infrastructure**, operated nationally.

Key uses include:

- **NHS smartcards**, which authenticate clinicians as known individuals.
- **Role-based access control**, where identity is established first and permissions applied separately.
- **Access to national services** such as demographic services and summary care records.
- **System-to-system communication** using mutual Transport Layer Security.
- **Formal electronic signatures** for legal or regulatory workflows.

VPR builds on this familiar model but applies X.509 certificates to **authorship of clinical record changes**, rather than to login or transport security.

---

## The patient’s voice

VPR supports both professional clinical entries and patient contributions within the same repository, using **distinct artefact paths**:

- `/clinical/` contains authoritative, professionally authored and signed records.
- `/patient/` contains patient-contributed material such as reported outcomes, symptom logs, or uploaded documents.

Patient input may inform clinical care, but it never overwrites clinical records without explicit professional review and a new signed commit.

This preserves patient voice while maintaining clinical accountability.

---

## Single-branch repository policy

Each VPR repository uses a single authoritative branch: `refs/heads/main`.

While Git itself allows multiple branches, VPR enforces a **single-branch policy** at the system level. Branches may exist transiently during local operations, but only `main` is accepted as authoritative.

This ensures a single, linear clinical history and avoids ambiguity about competing versions of truth.
