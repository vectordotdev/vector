fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../proto/vector/observability.proto");
    println!("cargo:rerun-if-changed=../../proto");

    tonic_build::configure()
        .build_server(false) // Client only, no server code generation
        .compile(
            &["../../proto/vector/observability.proto"],
            &["../../proto", "../../proto/third-party"],
        )?;
    Ok(())
}
