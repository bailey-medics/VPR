# VPR File Storage (Binary and Non-Text Files)

## Purpose

This document defines how non-text and binary files (for example PDFs, imaging, scans, waveforms, audio, and video) are stored, referenced, versioned, and governed within the Versioned Patient Repository (VPR).

The aim is to preserve clinical meaning, auditability, and long-term safety while remaining compatible with openEHR principles, offline use, and simple local operation (for example on a laptop), without introducing enterprise-only infrastructure.

---

## Core Principles

- Clinical meaning and binary bytes are deliberately separated
- Binary files are not tracked in Git
- Binary files are immutable once added (new content creates a new file)
- References to files are explicit, auditable, and versioned
- Clinical repositories remain valid even when binary files are absent
- No global or cross-repository binary namespace exists

---

## What Counts as a File

Files include, but are not limited to:

- Portable Document Format (PDF) documents
- Medical imaging (for example DICOM series)
- Scanned paper documents
- Audio or video recordings
- Physiological waveforms or monitoring exports

These files are treated as **clinical material**, but are not part of the primary structured clinical data.

---

## Repository-Scoped Storage Model

VPR does **not** use a global binary store.

Instead, **each repository is self-contained** and stores its own associated files alongside its versioned content.

This document describes the pattern using the **Clinical Repository (CR)** as the example. The same pattern applies independently to other repositories (CCR, DR, RRR).

---

## Clinical Repository Layout

For a single Clinical Repository:

```text
clinical/
└── <clinical_id>/
    ├── .gitignore
    ├── compositions/
    ├── indexes/
    ├── metadata/
    ├── … other CR-specific content …
    └── files/        # gitignored
```

### Invariants

- `<clinical_id>/` is the repository root and Git root
- The CR is independently portable and versioned
- `files/` is scoped **only** to this CR
- `files/` is explicitly excluded from Git tracking
- The CR remains valid even if `files/` is missing or incomplete

No patient identifier is implied or required by this structure.

---

## The `files/` Directory

The `files/` directory:

- Contains binary files associated with this Clinical Repository
- May include documents, imaging, video, audio, or other binary formats
- Is not required to be present on all copies of the repository
- Is never authoritative for clinical meaning

The name `files/` is intentionally neutral and does not imply format, size, or readability.

---

## File Identity and Integrity

Each file is identified by its **content**, not by its filename.

VPR implements content-addressed storage using SHA-256 hashes:

- Files are stored using their SHA-256 hash as the filename
- Two-level sharding is used to prevent excessive files per directory
- Hashes are used to verify integrity
- If file contents change, a new file is created

**Storage structure:**

```text
files/
└── sha256/
    └── ab/          # First 2 characters of hash
        └── cd/      # Next 2 characters of hash
            └── abcdef123456...  # Full hash as filename
```

---

## File References in the Clinical Repository

### Purpose of a File Reference

Clinical artefacts do not embed binary data.

Instead, they include **file references** which:

- Assert that a file exists or existed
- Describe the file’s clinical role
- Binds the reference immutably in time

File references are small, human-readable, and versioned as part of the CR.

---

### Typical Reference Metadata

A file reference records:

- Relative path to the file within `files/`
- Cryptographic hash (SHA-256)
- Hash algorithm identifier
- Media type (MIME type, best-effort detection)
- Original filename
- File size in bytes
- Storage timestamp (ISO 8601 format)

**Example (matching `FileMetadata` structure):**

```yaml
file_reference:
  hash_algorithm: sha256
  hash: abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890
  relative_path: files/sha256/ab/cd/abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890
  size_bytes: 1048576
  media_type: application/pdf
  original_filename: discharge-letter.pdf
  stored_at: "2026-01-24T10:30:00Z"
```

**Note:** The `media_type` is detected automatically using file content inspection and should not be considered authoritative for clinical purposes.

---

## Placement Rules

File references are stored **where the clinical meaning lives**:

- Letters, reports, results → referenced from CR artefacts
- Workflow or administrative material → referenced from CCR artefacts
- Withdrawn or redacted material → referenced from RRR artefacts

The origin of the file (patient, clinician, external organisation) does not determine placement.

Clinical meaning does.

---

## External and Patient-Provided Files

Patient-provided or externally received files follow a simple, explicit workflow:

1. The file is placed into the CR’s `files/` directory
2. A reference is created in an appropriate artefact
3. A clinician may later incorporate or reinterpret the material

This mirrors real-world clinical practice (for example “patient brought letter – reviewed”).

---

## Versioning Behaviour

- Files are immutable once added (enforced by the `FilesService`)
- New or corrected content results in a new file with a different hash
- References are append-only
- Historical references remain valid indefinitely
- Attempting to store a file with an existing hash returns an error

No reference is silently replaced or overwritten.

---

## Redaction and Removal

VPR does not support silent deletion.

When a file must be withdrawn or redacted:

- The reference in CR is explicitly marked as withdrawn or redacted
- A tombstone remains in versioned history
- The file may be removed from `files/` as a separate, explicit action

The system always retains evidence that the file once existed in the Redacted Retention Repository (RRR).

---

## Why Git Large File Storage Is Not Used

Git Large File Storage (LFS) is not suitable because:

- It relies on repository paths rather than actual content identity
- It complicates offline and partial copies
- It does not align with openEHR-style separation of meaning and identity

Git is used to version **clinical meaning**, not binary bytes.

---

## Enterprise Deployment and Acceleration (Non-Canonical Layer)

In enterprise deployments, VPR retains the on-disk Clinical Repository (CR) as the **canonical source of truth**, while performance, scale, and availability are achieved through **derived acceleration layers**. These include projection databases, indexes, and caches built by continuously reading the canonical CR and materialising fast read models for queries, lists, and search. Large files remain conceptually part of the CR but may be mirrored to object storage for durability and efficient delivery; such storage acts as a **distribution and persistence layer**, not a new authority. All enterprise components are explicitly rebuildable from the canonical repository, tolerate missing binary bytes, and never accept writes that bypass the CR. This preserves VPR’s laptop-first, openEHR-aligned philosophy while enabling high-throughput, low-latency operation at organisational scale.

---

## Implementation

VPR provides the `FilesService` (in the `vpr_files` crate) for managing binary file storage:

### Core Operations

- **`add(source_path)`** — Adds a file to content-addressed storage
  - Computes SHA-256 hash
  - Creates sharded storage path
  - Enforces immutability (errors if hash exists)
  - Detects media type automatically
  - Returns `FileMetadata` with all reference information

- **`read(hash)`** — Retrieves file contents by hash
  - Returns file as byte vector (`Vec<u8>`)
  - Suitable for network transmission
  - Errors if file not found

### Service Characteristics

- **Repository-scoped**: Each service instance is bound to one repository
- **Defensive**: Validates all paths and prevents directory traversal
- **Stateless**: No persistent state beyond filesystem
- **Safe**: All paths canonicalised to prevent symlink attacks

See `crates/files/src/files.rs` for complete implementation details.

---

## Summary

- Each repository stores its own files locally
- Files live in a `files/` directory alongside versioned content
- Files are not tracked by Git
- References are explicit, relative, and auditable
- Clinical meaning always lives in versioned artefacts

This design keeps VPR simple, portable, openEHR-aligned, and clinically honest.
