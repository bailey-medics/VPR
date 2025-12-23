//! # API Shared
//!
//! Shared utilities and definitions for VPR APIs.
//!
//! Contains:
//! - Protobuf-generated types (`pb` module)
//! - Shared services like `HealthService`
//! - Authentication utilities (usable by both gRPC and REST)
//!
//! Used by `api-grpc` and `api-rest` for common functionality.

// Re-export the generated protobuf module. The generated code will be placed
// into OUT_DIR at build time by the build script.
pub mod pb {
    tonic::include_proto!("vpr.v1");
}

pub mod auth;
pub mod health;

pub const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("proto_descriptor");

pub use health::HealthService;
pub use pb::*;
