use std::io::Result;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=proto/gogo.proto");
    println!("cargo:rerun-if-changed=proto/stats.proto");
    println!("cargo:rerun-if-changed=proto/logproto.proto");
    prost_build::compile_protos(
        &[
            "proto/gogo.proto",
            "proto/stats.proto",
            "proto/logproto.proto",
        ],
        &["proto", "../../proto"],
    )?;
    Ok(())
}
