// Re-export the proto module from the shared `api-proto` crate so callers
// can continue to reference `api::service::pb`.
pub use api_proto::pb;

// Re-export the service implementation type directly from the `core` crate.
// This ensures the type is publicly available as `api::service::VprService`.
pub use core::VprService;
