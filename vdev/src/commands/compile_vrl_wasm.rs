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
        let args = vec!["build", "--release", "--target", "wasm32-unknown-unknown"];
        println!("Compiling lib/vrl/compiler to wasm32-unknown-unknown");
        std::env::set_current_dir(vrl_path.join("compiler"))?;
        app::exec("cargo", &args, false)?;

        println!("Compiling lib/vrl/core to wasm32-unknown-unknown");
        std::env::set_current_dir(vrl_path.join("core"))?;
        app::exec("cargo", &args, false)?;

        println!("Compiling lib/vrl/diagnostic to wasm32-unknown-unknown");
        std::env::set_current_dir(vrl_path.join("diagnostic"))?;
        app::exec("cargo", &args, false)?;

        println!("Compiling lib/vrl/parser to wasm32-unknown-unknown");
        std::env::set_current_dir(vrl_path.join("parser"))?;
        app::exec("cargo", &args, false)?;
        Ok(())
    }
}
