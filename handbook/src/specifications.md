# VPR – Versioned Patient Repository

## Purpose

- Store patient records in a version controlled manner, using Git.
- Serve those records fast to clinicians, admins, or patients.
- Keep everything accurate, secure, and auditable.

## Technology Choices

- Rust for everything (fast, safe, compiled to a single binary).
- gRPC for the main API (fast, typed communication between systems).
- Postgres as the index (for lists and quick searches).
- Redis as a cache (to make repeated queries almost instant).
- Git as the underlying truth for documents (every version saved, nothing silently overwritten).

We’re choosing this stack to be as fast as possible while still safe and reliable.

## Data Model

- Records are stored as JSON files inside Git, versioned automatically.
- Each patient has their own folder, with subfolders for letters, messages, allergies, etc.
- Every new change makes a new version, never overwriting the old one.
- Postgres stores only metadata (like record type, patient ID, signed/draft flags, short titles, dates).
- Redis caches frequently-used lists and documents so they appear instantly.

## API

- Create a record – add a new JSON document for a patient.
- Update a record – add a new version of an existing document.
- Read a record – fetch the latest version (or a specific one).
- List records – get a quick list of documents for a patient or for tasks like “letters to sign”.
- Special queries – e.g. “show me all letters needing sign-off”.
- Reindex – rebuild the Postgres index if needed from Git.
- Health check – confirm the service is alive.

## Security

- All communication uses encryption (TLS).
- Only authorised systems can talk to VPR (via certificates or secure tokens).
- Data on disk can be encrypted if required.
- Patients downloading their data will always get it encrypted.

## Corrections & Deletions

- Normal use is append-only (you don’t delete history).
- If wrong patient data is added:
  - Prefer redaction (mark as wrong but leave audit trail).
  - If legally required, remove with a special process (cryptographic erase or repo rewrite).

## Performance Approach

- Lists always come from Postgres (never walk Git).
- Records are cached in Redis for quick access (LRU least recently used).
- Git is used only when a record is first opened or updated.
- Pre-compute titles, snippets, and “signed/draft” flags at save time so lists don’t need heavy work later.
- Background jobs prepare “likely to be used” data (e.g. tomorrow’s clinic letters) to make them instant.
- Shard directories by patient ID to improve Git performance.

## Reliability

- Every file is checksummed (SHA-256) for integrity.
- Provenance (who did what and when) is logged in both Git and Postgres.
- If Postgres or Redis fails, Git remains the single source of truth.
- A reindex job can always rebuild Postgres from Git.

## Operations

- Runs as one binary, easy to deploy.
- Configured by environment variables (DB URL, Redis URL, repo path).
- Git stored on fast SSD disks.
- Postgres and Redis tuned for speed and reliability.
- Backups: Git repo + Postgres dumps; can be replayed into a new instance.

##  Cargo features

- A feature flag for code builds.

Features needed for a patient to view and edit their own records:

```bash
cargo build --features patient
```

Features needed for clinicians and admins to manage records in a multi-patient environment:

```bash
cargo build --features org
```

## Wrong patient

- Redact
- Stub
  - Preserve cryptographic proof of what was removed
  - Hashed Message Authentication Code (mathematical fingerprint of the original data)
- Quarantine vault
  - Quarantine bytes
