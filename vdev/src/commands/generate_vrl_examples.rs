use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::app;

/// Generate VRL function examples from VRL stdlib and inject into docs.json
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// Dry run - don't write files, just print what would be done
    #[arg(long)]
    dry_run: bool,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::set_repo_dir()?;

        println!("Extracting VRL function examples from VRL stdlib...");

        let docs_json_path = Path::new("website/data/docs.json");

        if !docs_json_path.exists() {
            bail!("docs.json not found. Please run 'make -C website cue-build' first.");
        }

        // Read docs.json
        let docs_content = fs::read_to_string(docs_json_path)?;
        let mut docs: Value = serde_json::from_str(&docs_content)?;

        // Get all VRL functions and their examples
        let functions = vrl::stdlib::all();
        let mut functions_with_examples = BTreeMap::new();

        for function in functions {
            let function_name = function.identifier();
            let examples = function.examples();

            if !examples.is_empty() {
                let examples_vec: Vec<VrlExample> = examples
                    .iter()
                    .map(|ex| VrlExample {
                        title: ex.title.to_string(),
                        source: ex.source.to_string(),
                        result: match ex.result {
                            Ok(s) => Ok(s.to_string()),
                            Err(s) => Err(s.to_string()),
                        },
                    })
                    .collect();

                functions_with_examples.insert(function_name.to_string(), examples_vec);
            }
        }

        println!(
            "Found {} VRL functions with {} total examples",
            functions_with_examples.len(),
            functions_with_examples.values().map(|v| v.len()).sum::<usize>()
        );

        // Inject examples into docs.json
        let mut updated_count = 0;
        let mut skipped_count = 0;

        for (function_name, examples) in &functions_with_examples {
            // Navigate to remap.functions.<function_name>
            if let Some(function_obj) = docs
                .get_mut("remap")
                .and_then(|r| r.get_mut("functions"))
                .and_then(|f| f.get_mut(function_name))
            {
                // Get existing examples array
                if let Some(existing_examples) = function_obj.get_mut("examples") {
                    if let Some(examples_array) = existing_examples.as_array_mut() {
                        // Append new examples
                        for example in examples {
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
                                "[DRY RUN] Would append {} examples to {}",
                                examples.len(),
                                function_name
                            );
                        } else {
                            println!("✓ Appended {} examples to {}", examples.len(), function_name);
                        }
                        updated_count += 1;
                    }
                }
            } else {
                if !self.dry_run {
                    println!(
                        "⚠ Function not found in docs.json: {} (skipping)",
                        function_name
                    );
                }
                skipped_count += 1;
            }
        }

        println!("\nSummary:");
        println!("  Updated: {}", updated_count);
        println!("  Skipped: {}", skipped_count);

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

#[derive(Debug, Clone)]
struct VrlExample {
    title: String,
    source: String,
    result: Result<String, String>,
}
