use prost_wkt_build::*;
use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=proto/event.proto");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let descriptor_file = out.join("descriptors.bin");

    prost_build::Config::new()
        .protoc_arg("--experimental_allow_proto3_optional")
        .btree_map(["."])
        .bytes(["raw_bytes"])
        .type_attribute(".", "#[derive(serde::Serialize,serde::Deserialize)]")
        .extern_path(".google.protobuf.Any", "::prost_wkt_types::Any")
        .extern_path(".google.protobuf.Timestamp", "::prost_wkt_types::Timestamp")
        .extern_path(".google.protobuf.Value", "::prost_wkt_types::Value")
        .file_descriptor_set_path(&descriptor_file)
        .compile_protos(&["proto/event.proto"], &["proto", "../../proto"])
        .unwrap();

    let descriptor_bytes = std::fs::read(descriptor_file).unwrap();

    let descriptor = FileDescriptorSet::decode(&descriptor_bytes[..]).unwrap();

    prost_wkt_build::add_serde(out, descriptor);
}
