fn main() {
    println!("cargo:rerun-if-changed=proto/event.proto");
    prost_build::Config::new()
        .protoc_arg("--experimental_allow_proto3_optional")
        .btree_map(["."])
        .bytes(["raw_bytes"])
        .compile_protos(&["proto/event.proto"], &["proto", "../../proto"])
        .unwrap();
}
