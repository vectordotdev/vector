fn main() {
    println!("cargo:rerun-if-changed=proto/event.proto");
    let descriptor_path =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("event_descriptor.bin");
    let mut config = prost_build::Config::new();
    config
        .protoc_arg("--experimental_allow_proto3_optional")
        .btree_map(["."])
        .bytes(["raw_bytes"])
        .file_descriptor_set_path(descriptor_path)
        .compile_protos(
            &["proto/event.proto"],
            &["proto", "../../proto/third-party", "../../proto/vector"],
        )
        .unwrap();
}
