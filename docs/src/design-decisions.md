# Design decisions

This document captures the key architectural and governance decisions behind VPR, and the reasoning for each. The emphasis throughout is on auditability, clinical accountability, privacy, and long-term robustness.

File layouts can be seen at [openEHR file structure](open-ehr/index.md).

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

## Error handling: bespoke enums over anyhow

VPR uses **bespoke error enums** (for example `PatientError` in the core crate) rather than using `anyhow::Result` throughout.

This is a deliberate choice. In a clinical record system, failures are not just “an error message”: they often need to be handled consistently, audited, and mapped to user-facing outcomes.

### Why bespoke enums

- **Stable failure contract**: A named enum defines the set of failure modes VPR considers meaningful (for example invalid input, YAML parse failure, Git initialisation failure). This makes behaviour predictable as the code evolves.
- **Structured handling at boundaries**: API layers (gRPC/REST) can map specific error variants to appropriate status codes and responses without relying on string matching.
- **Better testability**: Tests can assert specific variants rather than brittle message strings, which improves confidence during refactors.
- **Separates domain intent from library detail**: An enum can express domain-relevant failures while still carrying underlying errors where useful.

### What we lose by not using anyhow everywhere

- **Less convenience**: `anyhow` is excellent for rapid development and rich, contextual error chains with minimal boilerplate.
- **More plumbing**: Explicit enums require writing variants and conversion/mapping code.

### Where anyhow can still be appropriate

At *application entrypoints* (for example a CLI binary), `anyhow` can still be a good fit for turning errors into high-quality diagnostics and an exit code. VPR keeps this style out of the core library surface so that upstream layers can make deterministic decisions based on typed errors.

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

---

## File format conventions in VPR

VPR uses different on-disk file formats depending on the **nature of the clinical information**, not based on technical fashion. The guiding principle is to optimise for **human readability, auditability, and safe review**, while remaining fully interoperable via APIs.

### Rule of thumb

**Choose the file format based on how the information is used and reviewed.**

- **Narrative clinical content**  
  (for example medical histories, clinic letters, discharge summaries, clinical reasoning)  
  → **Markdown with YAML front matter**

- **Structured clinical measurements**  
  (for example observations, blood tests, vital signs, scores)  
  → **YAML**

- **Machine-dense or high-volume data**  
  (for example large panels, waveforms, derived analytics outputs)  
  → **JSON, if needed**

- **APIs and external integrations**  
  → **JSON always**

### Rationale

- **Markdown** preserves clinical narrative, nuance, and intent, and produces clear, reviewable Git diffs.
- **YAML** is human-readable, diff-friendly, and well suited to structured clinical data that may need manual review or audit.
- **JSON** is optimal for transport and high-density machine processing, but less suitable for direct human review in Git.
- **APIs** standardise on JSON for interoperability and tooling compatibility.

This approach keeps clinical records legible to clinicians, robust under version control, and straightforward to serialise for external systems. The underlying data model remains the same regardless of file format; only the on-disk representation differs.

---

## Data flow and query model in VPR

VPR is designed around a clear separation between **clinical truth**, **performance**, and **user experience**. This separation is deliberate and underpins the system’s safety, auditability, and scalability.

### Canonical source of truth

VPR stores **clinical truth** in **Git-backed files**. These files (YAML and Markdown with YAML front matter) are the authoritative record of what was written, by whom, and when.

Git provides:

- a complete, immutable history of change
- authorship and provenance
- the ability to reconstruct record state at any point in time

These files are optimised for correctness, audit, and human review, not for fast querying.

### Interpretation into typed components

When files change, VPR:

1. Reads the updated files
2. Parses them into **typed Rust components** (the internal representation of clinical meaning)

These components are the semantic pivot of the system. They represent what the system *understands* clinically, independent of file format, Git, databases, or APIs.

### Projection into databases and caches

Typed components are then **projected** into databases and caches to support:

- indexing
- fast search
- filtering
- aggregation
- responsive user interfaces

Databases and caches store **derived representations**, not the canonical files themselves. They exist to answer questions efficiently, not to define truth.

### Serving user-facing queries

All interactive user queries are served from:

- databases
- search indexes
- caches

Git and on-disk files are not queried on the hot path. This keeps the user experience fast and predictable, even as the canonical record remains careful and auditable.

### CQRS principles in VPR

This architecture follows the core principles of **Command Query Responsibility Segregation (CQRS)**:

- **Commands (writes)**  
  Change clinical state by creating or modifying Git-backed files.  
  This path is slow, deliberate, validated, and fully auditable.

- **Queries (reads)**  
  Retrieve current, useful views of the data from database projections and caches.  
  This path is fast, flexible, and optimised for user needs.

The write model (files + Git) and the read model (databases + caches) are intentionally different and evolve independently.

### A useful mental model

A simple way to think about the system is:

> **Git-backed files describe what happened; databases describe what is currently useful to know.**

Both are essential. They answer different questions and are optimised for different purposes.

### Summary

- Git-backed files are the canonical clinical record
- Rust components represent interpreted clinical meaning
- Databases and caches provide fast, queryable projections
- User-facing queries never depend on Git or raw files
- CQRS-style separation keeps the system auditable, performant, and safe

This design allows VPR to combine strong clinical governance with a responsive modern user experience, without compromising either.
