use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(&["src/remote_config.proto"], &["src/"])?;
    Ok(())
}
