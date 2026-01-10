
# Git versioning and commit signatures

VPR stores each patient record as files on disk, and uses a **Git repository per patient directory** to version changes.
This enables history, diffs, and (optionally) cryptographic signing of commits.

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
