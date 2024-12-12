fn main() {
    println!("cargo:rerun-if-changed=proto/dnstap.proto");
    let mut prost_build = prost_build::Config::new();
    prost_build.btree_map(["."]);
    prost_build
        .compile_protos(&["proto/dnstap.proto"], &["proto"])
        .expect("Failed to compile proto files");
}
