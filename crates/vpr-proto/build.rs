fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The crate lives in `<repo>/crates/vpr-proto` so the workspace repo root
    // is two levels up from CARGO_MANIFEST_DIR.
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .ok_or("could not determine repository root from CARGO_MANIFEST_DIR")?;

    let proto_root = repo_root.join("proto");
    // Try the repo-level `proto/...` then fall back to `crates/api/proto/...`.
    let candidate1 = proto_root.join("vpr/v1/vpr.proto");
    let candidate2 = repo_root.join("crates/api/proto/vpr/v1/vpr.proto");

    let (proto_file, proto_include_root) = if candidate1.exists() {
        (candidate1, proto_root)
    } else if candidate2.exists() {
        (candidate2.clone(), repo_root.join("crates/api/proto"))
    } else {
        return Err(format!(
            "proto file not found. looked for:\n  {}\n  {}",
            candidate1.display(),
            candidate2.display()
        )
        .into());
    };

    let out_dir = std::env::var("OUT_DIR")?;
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .file_descriptor_set_path(std::path::Path::new(&out_dir).join("proto_descriptor.bin"))
        .compile_protos(
            std::slice::from_ref(&proto_file),
            &[proto_include_root.as_path()],
        )?;

    println!("cargo:rerun-if-changed={}", proto_file.display());
    Ok(())
}
