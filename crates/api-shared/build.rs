//! Build script for the `api-shared` crate.
//!
//! ## Purpose
//! Generates Rust protobuf types from `vpr.proto` and emits a file-descriptor set.
//!
//! ## Intended use
//! The generated types are shared by both gRPC and REST APIs. The descriptor set is used for gRPC
//! reflection.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let proto_file = std::path::Path::new(manifest_dir).join("vpr.proto");
    let proto_include_root = std::path::Path::new(manifest_dir);

    println!("cargo:rerun-if-changed={}", proto_file.display());
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .type_attribute(
            ".",
            "#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema)]",
        )
        .file_descriptor_set_path(
            std::path::Path::new(&std::env::var("OUT_DIR")?).join("proto_descriptor.bin"),
        )
        .compile_protos(std::slice::from_ref(&proto_file), &[proto_include_root])?;

    Ok(())
}
