use std::path::Path;
use anyhow::Result;
use clap::Args;

use crate::app;

/// Compiles VRL crates to wasm32-unknown-unknown
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let vrl_path = Path::new(app::path()).join("lib").join("vrl");
        let args = &["build", "--release", "--target", "wasm32-unknown-unknown"];

        for crate_name in  ["compiler", "core", "diagnostic", "parser"] {
            println!("Compiling lib/vrl/{crate_name} to wasm32-unknown-unknown");
            std::env::set_current_dir(vrl_path.join(crate_name))?;
            app::exec("cargo", *args, false)?;
        }
        Ok(())
    }
}
