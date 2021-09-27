fn main() {
    println!("cargo:rerun-if-changed=proto/event.proto");
    let _ = prost_build::Config::new()
        .btree_map(&["."])
        .bytes(&["raw_bytes"])
        .compile_protos(&["proto/event.proto"], &["proto/"])
        .unwrap();
}
