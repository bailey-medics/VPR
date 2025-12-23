//! # API REST
//!
//! REST API implementation for VPR.
//!
//! Handles:
//! - HTTP endpoints with axum
//! - OpenAPI/Swagger documentation
//! - REST-specific concerns (JSON serialization, CORS)
//!
//! Uses `api-shared` for common types and utilities.

#![warn(rust_2018_idioms)]

pub use core::VprService;
