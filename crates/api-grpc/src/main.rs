use std::net::SocketAddr;
use tonic::transport::Server;
use tonic_reflection::server::Builder;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use api_grpc::{pb::vpr_server::VprServer, VprService};
use api_proto::FILE_DESCRIPTOR_SET;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env().add_directive("vpr=info".parse()?))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let addr: SocketAddr = std::env::var("VPR_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".into())
        .parse()?;

    tracing::info!("-- Starting VPR gRPC on {}", addr);

    let svc = VprService;
    let mut server_builder = Server::builder().add_service(VprServer::new(svc));

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
