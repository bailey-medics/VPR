# Git versioning and commit signatures

VPR stores each patient record as files on disk, and uses a **Git repository per patient directory** to version changes.
This enables history, diffs, and (optionally) cryptographic signing of commits.

## Immutability and Audit Trail Philosophy

### Core Principle: Nothing is Ever Deleted

VPR maintains a **completely immutable audit trail**. Nothing is ever truly deleted from the version control history.
This fundamental design choice ensures:

- **Patient Safety**: Every change is traceable to a specific author at a specific time
- **Legal Compliance**: Complete audit trail meets regulatory requirements
- **Clinical Governance**: Full accountability for all modifications
- **Research and Quality**: Historical data remains available for authorized retrospective analysis

### Commit Actions and Their Meaning

VPR uses a controlled vocabulary for commit actions, each with specific semantics:

#### `Create`

Used when adding new content to an existing record. Examples:

- Creating a new clinical letter
- Adding a new observation
- Recording a new diagnosis
- Initializing a new patient record

This is the most common action for new data entry.

#### `Update`

Used when modifying existing content. Examples:

- Correcting a typo in a letter
- Updating patient demographics (address change, name change)
- Linking demographics to clinical records
- Amending administrative details

The previous version remains in Git history and can be compared via diff.

#### `Superseded`

Used when newer clinical information makes previous content obsolete. Examples:

- A revised diagnosis based on new test results
- An updated care plan
- Replacement of preliminary findings with final results

This is distinct from `Update` as it represents a clinical decision that previous information
should be replaced rather than corrected. The superseded content remains in history but is
marked as no longer current for clinical decision-making.

#### `Redact`

Used when data was entered into the wrong patient's repository by mistake. This can occur
in any of the three repositories: clinical, demographics, or coordination. This is the
**only action that removes data from active view**. The process:

1. Data is removed from the patient's active record
2. Data is encrypted and moved to the Redaction Retention Repository
3. A non-human-readable tombstone/pointer remains in the Git history
4. The commit message records the redaction action for audit purposes

Even redacted data is preserved in secure storage and remains accessible to authorized
auditors, ensuring complete traceability while protecting patient privacy.

### What This Means in Practice

- **Every change is preserved**: Git commits form an unbroken chain from initialization to present
- **Diffs show what changed**: You can compare any two points in time
- **Authors are accountable**: Each commit is signed (optionally cryptographically) with author metadata
- **No data loss**: Even mistakes are preserved in history, allowing forensic analysis if needed
- **Audit compliance**: Regulators can verify that no data has been improperly deleted

## Where Git repos live

Clinical records are stored under the sharded directory structure:

- `patient_data/clinical/<s1>/<s2>/<32-hex-uuid>/`

That patient directory is initialised as a Git repository (`.git/` lives inside it).

## Initial commit creation

When a new clinical record is created:

1. VPR copies the `clinical-template/` directory into the patient directory.
2. VPR writes the initial `ehr_status.yaml`.
3. VPR stages all files (excluding `.git/`) and writes a tree.
4. VPR creates the initial commit.

The implementation lives in [crates/core/src/clinical.rs](../../crates/core/src/clinical.rs) in `ClinicalService::initialise`.

## Branch behaviour (`main`)

Signed commits are created with `git2::Repository::commit_signed`. A key detail of libgit2 is:

- `commit_signed` **creates the commit object** but **does not update any refs** (no branch ref, no `HEAD` update).

To ensure the repo behaves like a normal Git repo, VPR explicitly:

- sets `HEAD` to `refs/heads/main` before creating the first commit, and
- after the signed commit is created, creates/updates `refs/heads/main` to point at that commit and points `HEAD` to it.

Result: clinical repos “land on” the `main` branch.

## How signing works

If `Author.signature` is provided during initialisation, VPR signs the initial commit using ECDSA P-256.

- Payload: the **unsigned commit buffer** produced by `Repository::commit_create_buffer`.
  - This is the exact byte payload that must be signed to match what `commit_signed` expects.
- Algorithm: ECDSA over P-256 (`p256` crate).
- Signature encoding:
  - VPR uses the raw 64-byte signature format (`r || s`) and base64-encodes it.
  - This base64 string is passed to `commit_signed` and ends up stored in the commit header field `gpgsig`.

Notes:

- Despite the `gpgsig` name, this is not a GPG signature; it is an ECDSA signature stored in that header field.
- VPR currently focuses on “is this commit cryptographically valid for this key?”, not on GPG identity chains.

## How verification works

VPR can verify that a commit was signed by the private key corresponding to a provided public key.

Verification steps (implemented in `ClinicalService::verify_commit_signature`):

1. Open the patient Git repo.
2. Resolve the latest commit from `HEAD`.
3. Read the `gpgsig` header field from the commit.
4. Normalise it (handle whitespace wrapping), base64-decode it, and parse as a P-256 ECDSA signature.
5. Recreate the unsigned commit buffer with `commit_create_buffer` using the commit’s tree/parents/author/committer/message.
6. Verify the signature over that recreated buffer using the provided public key.

Important behaviour:

- Verification currently requires a valid `HEAD` (it does not attempt to recover commits from an unborn branch).
- The verifier accepts either:
  - a PEM-encoded public key, or
  - a PEM-encoded X.509 certificate (`.crt`), in which case the EC public key is extracted and used.

## CLI usage

The CLI exposes verification as:

- `vpr verify-clinical-commit-signature <clinical_uuid> <public_key_or_cert>`

Examples:

- `vpr verify-clinical-commit-signature 572ae9ebde8c480ba20b359f82f6c2e7 dr_smith.crt`
- `vpr verify-clinical-commit-signature 572ae9ebde8c480ba20b359f82f6c2e7 ./dr_smith_public_key.pem`

## What this does (and does not) prove

This verification proves:

- the commit’s signature is mathematically valid for the provided public key, over the exact commit payload VPR signs.

It does not (by itself) prove:

- that a certificate is trusted (no chain/CA validation),
- that the author identity is “real” (it’s still a local signature check).
