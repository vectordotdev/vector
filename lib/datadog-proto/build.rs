use std::{
    fs::{read_to_string, write},
    io::Result,
    path::{Path, PathBuf},
};

fn main() -> Result<()> {
    for path in [
        "proto/datadog/agentpayload.proto",
        "proto/datadog/trace/agent_payload.proto",
        "proto/datadog/trace/tracer_payload.proto",
        "proto/datadog/trace/span.proto",
        "proto/datadog/trace/idx/tracer_payload.proto",
        "proto/datadog/trace/idx/span.proto",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let descriptor_path = out_dir.join("datadog-proto.desc");

    let mut prost_build = prost_build::Config::new();
    prost_build
        .btree_map(["."])
        .file_descriptor_set_path(&descriptor_path);

    prost_build.compile_protos(
        &[
            "proto/datadog/agentpayload.proto",
            "proto/datadog/trace/agent_payload.proto",
            "proto/datadog/trace/idx/tracer_payload.proto",
        ],
        &["proto"],
    )?;

    write_static_descriptor_reference(&descriptor_path, &out_dir)
}

fn write_static_descriptor_reference(descriptor_path: &Path, out_dir: &Path) -> Result<()> {
    let include_line = format!(
        r#"/// Raw file descriptor set for the Datadog protobuf packages in this crate.
pub static DESCRIPTOR_BYTES: &[u8] = include_bytes!(r"{}");
"#,
        descriptor_path.display()
    );

    let include_file = out_dir.join("datadog-proto.rs");
    let existing = read_to_string(&include_file).ok();
    if existing.as_deref() != Some(&include_line) {
        write(&include_file, include_line)?;
    }

    Ok(())
}
