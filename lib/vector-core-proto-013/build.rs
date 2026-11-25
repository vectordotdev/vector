fn main() {
    println!("cargo:rerun-if-changed=../vector-core/proto/event.proto");
    prost_build::Config::new()
        .protoc_arg("--experimental_allow_proto3_optional")
        .btree_map(["."])
        .bytes(["raw_bytes"])
        .compile_protos(
            &["../vector-core/proto/event.proto"],
            &["../vector-core/proto", "../../proto/third-party"],
        )
        .unwrap();
}
