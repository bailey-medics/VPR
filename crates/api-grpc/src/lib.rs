//! # API gRPC
//!
//! gRPC server implementation for VPR.
//!
//! Handles:
//! - gRPC service setup and authentication
//! - Service implementations using `vpr-core` for data operations
//! - gRPC-specific concerns (interceptors, tonic integration)
//!
//! Uses `api-shared` for common types and utilities.

#![warn(rust_2018_idioms)]

pub use service::{auth_interceptor, pb, VprService};

pub mod service;
