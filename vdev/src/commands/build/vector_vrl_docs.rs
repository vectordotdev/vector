use anyhow::Result;
use std::path::PathBuf;
use vrl::docs::{build_functions_doc, document_functions_to_dir};

/// Generate Vector-specific VRL function documentation as JSON files.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Output directory to create JSON files. If unspecified output is written to stdout as a JSON
    /// array
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Whether to pretty-print or minify
    #[arg(short, long, default_value_t = false)]
    minify: bool,

    /// File extension for generated files
    #[arg(short, long, default_value = "json")]
    extension: String,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let functions = vector_vrl_functions::all_without_vrl_stdlib();
        if let Some(output) = &self.output {
            document_functions_to_dir(&functions, output, &self.extension)?;
        } else {
            let built = build_functions_doc(&functions);
            #[allow(clippy::print_stdout)]
            if self.minify {
                println!(
                    "{}",
                    serde_json::to_string(&built)
                        .expect("FunctionDoc serialization should not fail")
                );
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&built)
                        .expect("FunctionDoc serialization should not fail")
                );
            }
        }
        Ok(())
    }
}
