fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto/vector/observability.proto");
    println!("cargo:rerun-if-changed=../../proto");
    println!("cargo:rerun-if-changed=../../lib/vector-core/proto/event.proto");

    // First, generate event.proto types
    tonic_build::configure().build_server(false).compile(
        &["../../lib/vector-core/proto/event.proto"],
        &["../../lib/vector-core/proto", "../../proto/third-party"],
    )?;

    // Then, generate observability.proto using extern_path to reference the event types
    let mut prost_config = prost_build::Config::new();
    prost_config.extern_path(".event", "crate::proto::event");
    // Allow clippy warning for large enum variant in generated code
    // TappedEvent contains a large EventWrapper while EventNotification is small
    prost_config.type_attribute(
        ".vector.observability.v1.StreamOutputEventsResponse",
        "#[allow(clippy::large_enum_variant)]",
    );

    tonic_build::configure()
        .build_server(false)
        .compile_with_config(
            prost_config,
            &["../../proto/vector/observability.proto"],
            &[
                "../../proto",
                "../../proto/third-party",
                "../../lib/vector-core/proto",
            ],
        )?;
    Ok(())
}
