fn main() {
    println!("cargo:rerun-if-changed=proto/event.proto");
    let mut prost_build = prost_build::Config::new();
    prost_build.btree_map(&["."]);
    prost_build
        .compile_protos(&["proto/event.proto"], &["proto/"])
        .unwrap();
    built::write_built_file().expect("Failed to acquire build-time information");
}
