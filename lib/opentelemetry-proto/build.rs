use std::{
    fs::{read_to_string, write},
    io::Result,
    path::{Path, PathBuf},
};

use glob::glob;

fn main() -> Result<()> {
    let proto_root = PathBuf::from("src/proto/opentelemetry-proto");
    let include_path = proto_root.clone();

    let proto_paths: Vec<_> = glob(&format!("{}/**/*.proto", proto_root.display()))
        .expect("Failed to read glob pattern")
        .filter_map(|result| result.ok())
        .collect();

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let descriptor_path = out_dir.join("opentelemetry-proto.desc");

    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .file_descriptor_set_path(&descriptor_path)
        .compile(&proto_paths, &[include_path])?;

    write_static_descriptor_reference(&descriptor_path, &out_dir)?;

    Ok(())
}

fn write_static_descriptor_reference(descriptor_path: &Path, out_dir: &Path) -> Result<()> {
    let include_line = format!(
        "pub static DESCRIPTOR_BYTES: &[u8] = include_bytes!(r\"{}\");\n",
        descriptor_path.display()
    );

    let include_file = out_dir.join("opentelemetry-proto.rs");
    let existing = read_to_string(&include_file).ok();
    if existing.as_deref() != Some(&include_line) {
        write(&include_file, include_line)?;
    }

    Ok(())
}
