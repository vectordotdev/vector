use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::{Value, json};

use crate::app;

/// Generate VRL function examples from VRL stdlib and inject into docs.json
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// Dry run - don't write files, just print what would be done
    #[arg(long)]
    dry_run: bool,
}

// FIXME this shouldn't exist, all functions should be documented
static UNDOCUMENTED_FNS: [&str; 6] = [
    "dns_lookup",
    "http_request",
    "reverse_dns",
    "tally",
    "tally_value",
    "type_def",
];

// FIXME this shouldn't exist, all functions should have examples
static NO_EXAMPLES_FNS: [&str; 1] = ["strip_ansi_escape_codes"];

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::set_repo_dir()?;

        println!("Extracting VRL function examples from VRL stdlib...");

        let docs_json_path = Path::new("website/data/docs.json");

        ensure!(
            docs_json_path.exists(),
            "docs.json not found. Please run 'make -C website cue-build' first."
        );

        // Read docs.json
        let docs_content = fs::read_to_string(docs_json_path)?;
        let mut docs: Value = serde_json::from_str(&docs_content)?;

        // Get all VRL functions and their examples
        let functions = vrl::stdlib::all();
        let mut functions_with_examples = BTreeMap::new();

        for function in functions {
            let function_name = function.identifier();
            let examples = function.examples();

            if UNDOCUMENTED_FNS.contains(&function_name) {
                continue;
            }

            if !NO_EXAMPLES_FNS.contains(&function_name) {
                assert!(!examples.is_empty(), "{function_name} has no examples!");
            }

            functions_with_examples.insert(function_name.to_string(), function);
        }

        println!(
            "Found {} VRL functions with {} total examples",
            functions_with_examples.len(),
            functions_with_examples
                .values()
                .map(|v| v.examples().len())
                .sum::<usize>()
        );

        // Inject examples into docs.json
        for (function_name, function) in &functions_with_examples {
            // Navigate to remap.functions.<function_name>
            let function_obj = docs
                .get_mut("remap")
                .and_then(|r| r.get_mut("functions"))
                .and_then(|f| f.get_mut(function_name))
                .with_context(|| {
                    format!("⚠ VRL function not found in docs.json: {function_name}")
                })?;

            let examples_array = {
                if function_obj.get("examples").is_none() {
                    function_obj
                        .as_object_mut()
                        .with_context(|| {
                            format!("{function_name} remap.functions is not an object")
                        })?
                        .insert("examples".to_string(), Value::Array(vec![]));
                }

                let existing_examples = function_obj.get_mut("examples").unwrap();
                existing_examples
                    .as_array_mut()
                    .with_context(|| format!("{function_name} examples is not an array"))?
            };

            // Append new examples
            for example in function.examples() {
                let mut example_json = json!({
                    "title": example.title,
                    "source": example.source,
                });

                match &example.result {
                    Ok(value) => {
                        // Remove VRL string literal syntax if present
                        let clean_value = if value.starts_with("s'") && value.ends_with('\'') {
                            &value[2..value.len() - 1]
                        } else {
                            value
                        };
                        example_json["return"] = json!(clean_value);
                    }
                    Err(error) => {
                        example_json["error"] = json!(error);
                    }
                }

                examples_array.push(example_json);
            }

            if self.dry_run {
                println!(
                    "[DRY RUN] Would append {} examples to {function_name}",
                    function.examples().len()
                );
            } else {
                println!(
                    "✓ Appended {} examples to {function_name}",
                    function.examples().len()
                );
            }
        }

        if self.dry_run {
            println!("\n(This was a dry run - no files were modified)");
        } else {
            // Write back to docs.json
            let updated_json = serde_json::to_string(&docs)?;
            fs::write(docs_json_path, updated_json)?;
            println!("\n✓ Updated docs.json");
        }

        Ok(())
    }
}
