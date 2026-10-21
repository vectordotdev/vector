fn main() {
    println!("cargo:rerun-if-changed=proto/prometheus-remote.proto");
    println!("cargo:rerun-if-changed=proto/prometheus-types.proto");
    let mut prost_build = prost_build::Config::new();
    prost_build.btree_map(["."]);
    prost_build.type_attribute("Label", "#[derive(Ord, PartialOrd)]");
    prost_build
        .compile_protos(
            &["proto/prometheus-remote.proto"],
            &["proto", "../../proto/third-party"],
        )
        .unwrap();
}
