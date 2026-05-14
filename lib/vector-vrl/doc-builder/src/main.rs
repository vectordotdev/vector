use anyhow::Result;
use std::path::PathBuf;
use vrl::docs::{build_functions_doc, document_functions_to_dir};

/// Generate Vector-specific VRL function documentation as JSON files.
///
/// Two modes of operation:
///   --output <DIR>              Write one JSON file per function into DIR (uses --extension).
///   (no --output)               Print all functions to stdout as a JSON array (uses --minify).
#[derive(clap::Parser, Debug)]
#[command()]
struct Cli {
    /// Output directory to create JSON files. When omitted, output is written to stdout as a JSON
    /// array.
    #[arg(short, long, conflicts_with = "minify")]
    output: Option<PathBuf>,

    /// Whether to minify the JSON output (stdout mode only)
    #[arg(short, long, default_value_t = false, conflicts_with = "output")]
    minify: bool,

    /// File extension for generated files (directory mode only)
    #[arg(short, long, default_value = "json", requires = "output")]
    extension: String,
}

#[allow(clippy::print_stdout)]
fn main() -> Result<()> {
    let cli = <Cli as clap::Parser>::parse();
    let functions = vector_vrl_functions::all_without_vrl_stdlib();
    if let Some(output) = &cli.output {
        document_functions_to_dir(&functions, output, &cli.extension)?;
    } else {
        let built = build_functions_doc(&functions);
        if cli.minify {
            println!(
                "{}",
                serde_json::to_string(&built).expect("FunctionDoc serialization should not fail")
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
