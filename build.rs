fn main() {
    built::write_built_file().unwrap();

    tower_grpc_build::Config::new()
        .enable_server(true)
        .enable_client(true)
        // This will build both `event.proto` and `vector.proto`
        // since `vector.proto` depends on `event.proto`
        .build(&["proto/vector.proto"], &["proto/"])
        .unwrap_or_else(|e| panic!("protobuf compilation failed: {}", e));

    println!("cargo:rerun-if-changed=proto/event.proto");
}
