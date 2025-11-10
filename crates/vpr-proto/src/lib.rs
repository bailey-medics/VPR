// Re-export the generated protobuf module. The generated code will be placed
// into OUT_DIR at build time by the build script.
pub mod pb {
    tonic::include_proto!("vpr.v1");
}

pub const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("proto_descriptor");

pub use pb::*;
