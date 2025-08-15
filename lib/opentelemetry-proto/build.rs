use glob::glob;
use std::{env, io::Result, path::PathBuf};

fn main() -> Result<()> {
    let proto_root = PathBuf::from("src/proto/opentelemetry-proto");
    let include_path = proto_root.clone();

    let proto_paths: Vec<_> = glob(&format!("{}/**/*.proto", proto_root.display()))
        .expect("Failed to read glob pattern")
        .filter_map(|result| result.ok())
        .collect();

    // Set up re-run triggers
    for proto in &proto_paths {
        println!("cargo:rerun-if-changed={}", proto.display());
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let descriptor_path = out_dir.join("opentelemetry-proto.desc");

    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .file_descriptor_set_path(&descriptor_path)
        .compile(&proto_paths, &[include_path])?;

    Ok(())
}
