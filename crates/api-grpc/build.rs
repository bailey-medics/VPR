//! Build script for the `api-grpc` crate.
//!
//! ## Purpose
//! This crate does not currently generate code at build time.
//!
//! ## Intended use
//! Historically, protobuf generation lived in this crate. The generated types now come from the
//! shared `api-shared` crate, so this build script is intentionally a no-op.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
