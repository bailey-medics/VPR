//! Standalone gRPC server binary.
//!
//! ## Purpose
//! Runs the gRPC API server on its own.
//!
//! ## Intended use
//! This binary is useful for development and debugging when you only want the gRPC server (for
//! example, with reflection enabled). The workspace's main `vpr-run` binary runs both gRPC and
//! REST concurrently.

use std::net::SocketAddr;
use tonic::transport::Server;
use tonic_reflection::server::Builder;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use api_grpc::{auth_interceptor, pb::vpr_server::VprServer, VprService};
use api_shared::FILE_DESCRIPTOR_SET;
use std::path::Path;
use std::sync::Arc;
use vpr_core::config::rm_system_version_from_env_value;
use vpr_core::CoreConfig;

/// Main entry point for the VPR gRPC server
///
/// Starts the gRPC server on the configured address (default: 0.0.0.0:50051).
/// Includes authentication interceptor and optional gRPC reflection for debugging.
///
/// # Environment Variables
/// - `VPR_ADDR`: Server address (default: "0.0.0.0:50051")
/// - `VPR_ENABLE_REFLECTION`: Enable gRPC reflection (default: "false")
/// - `API_KEY`: API key for authentication
///
/// # Returns
/// * `Ok(())` - If server starts and runs successfully
///
/// # Errors
/// Returns an error if:
/// - the logging/tracing configuration cannot be initialised,
/// - the server address cannot be parsed,
/// - the gRPC server cannot be bound or started.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let patient_data_dir = std::env::var("PATIENT_DATA_DIR")
        .unwrap_or_else(|_| vpr_core::DEFAULT_PATIENT_DATA_DIR.into());
    let patient_data_path = Path::new(&patient_data_dir);
    if !patient_data_path.exists() {
        anyhow::bail!(
            "Patient data directory does not exist: {}",
            patient_data_path.display()
        );
    }

    let rm_system_version = rm_system_version_from_env_value(
        std::env::var("RM_SYSTEM_VERSION")
            .ok()
            .and_then(|s| vpr_core::NonEmptyText::new(s).ok()),
    )?;

    let vpr_namespace = std::env::var("VPR_NAMESPACE")
        .ok()
        .and_then(|s| vpr_core::NonEmptyText::new(s).ok())
        .unwrap_or_else(|| vpr_core::NonEmptyText::new("vpr.dev.1").unwrap());

    let cfg = Arc::new(CoreConfig::new(
        patient_data_path.to_path_buf(),
        rm_system_version,
        vpr_namespace,
    )?);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env().add_directive("vpr=info".parse()?))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let addr: SocketAddr = std::env::var("VPR_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".into())
        .parse()?;

    tracing::info!("-- Starting VPR gRPC on {}", addr);

    let svc = VprService::new(cfg);
    let mut server_builder =
        Server::builder().add_service(VprServer::with_interceptor(svc, auth_interceptor));

    if std::env::var("VPR_ENABLE_REFLECTION").unwrap_or_else(|_| "false".to_string()) == "true" {
        let reflection_service = Builder::configure()
            .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
            .build_v1()
            .unwrap();
        server_builder = server_builder.add_service(reflection_service);
        tracing::info!("gRPC server reflection enabled");
    } else {
        tracing::info!("gRPC server reflection disabled");
    }

    server_builder.serve(addr).await?;

    Ok(())
}
