# Concurrency and Correctness in VPR

## Purpose

VPR is a file-based patient record system where each patient record is stored in its own Git repository (for example containing files such as `ehr_status.yaml`).

In a production deployment, multiple worker processes and multiple servers may handle requests concurrently. This document explains the simple, robust approach used by VPR to ensure:

- Only one update to a patient record happens at a time
- No updates are lost
- Git repositories are never left in an inconsistent or partially-written state
- The system remains safe across crashes and restarts

The design intentionally favours correctness and clarity over complexity.

---

## Core Principle

**For any given patient, only one writer is allowed at a time, and every write is checked before it is saved.**

This is achieved using two layers:

1. Per-patient serialisation (to decide whose turn it is to write)
2. Optimistic concurrency checks at the Git layer (to prevent lost updates)

---

## Layer 1: Per-Patient Serialisation

### Problem

In a clustered environment, two workers may attempt to update the same patient record at the same time.

### Solution

Before making any change, a worker must acquire a **per-patient lock** from a shared, trusted service (typically the main relational database).

- The lock is keyed by patient identifier
- Only one worker can hold the lock at a time
- Different patients can be updated in parallel
- If a worker crashes, the lock is automatically released

This guarantees that, at the system level, only one writer is active for a given patient at any moment.

### Mental Model

- The database acts as a traffic light
- Green means "you may edit this patient now"
- Red means "wait or retry later"

---

## Layer 2: Git-Based Optimistic Concurrency

### Problem

Even with serialisation, extra protection is needed to ensure a write does not overwrite a newer version of the record.

### Solution

Git already provides a perfect version check.

- When a worker reads a patient repository, it records the current commit hash
- When pushing an update, the worker asserts that the repository is still at that commit
- If the repository has moved on, the push is rejected

This prevents:

- Lost updates
- Silent overwrites
- Inconsistent repository state

### Mental Model

> "Only save my changes if nothing has changed since I last looked."

---

## End-to-End Write Flow

For a single patient update, the system follows this sequence:

1. Acquire the per-patient lock from the shared database
2. Read the patient Git repository and record the current commit hash
3. Apply changes locally in an isolated working copy
4. Create a Git commit containing the update
5. Push the commit, asserting the expected previous commit hash
6. Release the per-patient lock

If any step fails, the operation is retried or aborted safely without corrupting the patient record.

---

## Failure and Crash Safety

The system is designed so that failures are safe by default.

- If a worker crashes before pushing, the repository is unchanged
- If a worker crashes after pushing, the change is already complete
- Locks are not permanent and are released automatically
- Git guarantees atomic updates of repository state

No manual intervention is required to recover from partial failures.

---

## What This Design Guarantees

- Exactly one writer per patient at any given time
- No lost or overwritten updates
- No partially-written files
- Safe operation across multiple machines
- Simple, auditable behaviour

---

## What This Design Intentionally Avoids

At the current scale, VPR does not require:

- Distributed consensus systems
- Message queues for write coordination
- Shared filesystem locks
- Complex conflict resolution logic

These may be introduced later if throughput demands increase, but are not necessary for correctness.

---

## Summary

VPR ensures correctness by combining:

- A shared per-patient lock to serialise writes
- Git commit checks to prevent overwriting newer data

This approach is intentionally boring, und
