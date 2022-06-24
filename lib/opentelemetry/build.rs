fn main() {
    {
        let mut prost_build = prost_build::Config::new();
        prost_build.btree_map(&["."]);

        tonic_build::configure()
            .compile_with_config(
                prost_build,
                &["opentelemetry/proto/collector/logs/v1/logs_service.proto"],
                &["."],
            )
            .unwrap();
    }
}
