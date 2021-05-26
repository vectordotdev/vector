fn main() {
    #[cfg(any(
        feature = "sources-vector",
        feature = "sources-dnstap",
        feature = "sinks-vector"
    ))]
    {
        println!("cargo:rerun-if-changed=proto/vector.proto");
        println!("cargo:rerun-if-changed=proto/dnstap.proto");

        let mut prost_build = prost_build::Config::new();
        prost_build.btree_map(&["."]);

        tonic_build::configure()
            .compile_with_config(
                prost_build,
                &[
                    "lib/vector-core/proto/event.proto",
                    "proto/vector.proto",
                    "proto/dnstap.proto",
                ],
                &["proto/", "lib/vector-core/proto/"],
            )
            .unwrap();
    }

    built::write_built_file().expect("Failed to acquire build-time information");
}
