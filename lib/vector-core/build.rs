fn main() {
    println!("cargo:rerun-if-changed=proto/event.proto");
    prost_build::Config::new()
        .btree_map(&["."])
        .bytes(&["raw_bytes"])
        .compile_protos(&["proto/event.proto"], &["proto", "../../proto"])
        .unwrap();
}
