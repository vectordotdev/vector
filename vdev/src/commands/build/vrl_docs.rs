use anyhow::Result;
use indexmap::IndexMap;
use serde::Serialize;
use std::{fs, path::PathBuf};
use vrl::docs::{FunctionDoc, build_functions_doc};

/// Generate VRL function documentation as JSON files.
///
/// This command iterates over all VRL functions available in Vector and VRL and
/// generates a generated.cue documentation file with all functions' documentation.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Output directory for generated documentation files
    #[arg(long, default_value = "website/cue/reference/remap/functions")]
    output_dir: PathBuf,
}

#[derive(Serialize)]
struct FunctionDocWrapper {
    remap: RemapWrapper,
}

#[derive(Serialize)]
struct RemapWrapper {
    functions: IndexMap<String, FunctionDoc>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let functions = vector_vrl_functions::all();

        let docs = build_functions_doc(&functions);
        let functions_map = docs
            .into_iter()
            .map(|doc| (doc.name.clone(), doc))
            .collect();

        let wrapper = FunctionDocWrapper {
            remap: RemapWrapper {
                functions: functions_map,
            },
        };

        // Ensure output directory exists
        fs::create_dir_all(&self.output_dir)?;

        let mut json = serde_json::to_string(&wrapper)?;
        json.push('\n');
        let filepath = self.output_dir.join("generated.cue");
        fs::write(&filepath, json)?;

        println!("Generated: {}", filepath.display());

        println!("\nVRL documentation generation complete.");
        Ok(())
    }
}
