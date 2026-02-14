use anyhow::Result;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use vrl::compiler::Function;
use vrl::compiler::value::kind;
use vrl::core::Value;

/// Generate VRL function documentation as JSON files.
///
/// This command iterates over all VRL functions available in Vector and generates
/// JSON documentation files that are compatible with the CUE-based documentation
/// pipeline (valid JSON is valid CUE).
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
    functions: std::collections::HashMap<String, FunctionDoc>,
}

#[derive(Serialize)]
struct FunctionDoc {
    anchor: String,
    name: String,
    category: String,
    description: String,
    arguments: Vec<ArgumentDoc>,
    r#return: ReturnDoc,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    internal_failure_reasons: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    examples: Vec<ExampleDoc>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    notices: Vec<String>,
    pure: bool,
}

#[derive(Serialize)]
struct ArgumentDoc {
    name: String,
    description: String,
    required: bool,
    r#type: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<String>,
}

#[derive(Serialize)]
struct ReturnDoc {
    types: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    rules: Vec<String>,
}

#[derive(Serialize)]
struct ExampleDoc {
    title: String,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#return: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    raises: Option<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let functions = vector_vrl_functions::all();

        // Ensure output directory exists
        fs::create_dir_all(&self.output_dir)?;

        for func in functions {
            let doc = build_function_doc(func.as_ref());
            let filename = format!("{}.cue", doc.name);
            let filepath = self.output_dir.join(&filename);

            // Wrap in the expected CUE structure
            let mut functions_map = std::collections::HashMap::new();
            functions_map.insert(doc.name.clone(), doc);
            let wrapper = FunctionDocWrapper {
                remap: RemapWrapper {
                    functions: functions_map,
                },
            };

            let json = serde_json::to_string_pretty(&wrapper)?;
            fs::write(&filepath, json)?;

            println!("Generated: {}", filepath.display());
        }

        println!("\nVRL documentation generation complete.");
        Ok(())
    }
}

fn build_function_doc(func: &dyn Function) -> FunctionDoc {
    let name = func.identifier().to_string();

    let arguments: Vec<ArgumentDoc> = func
        .parameters()
        .iter()
        .map(|param| ArgumentDoc {
            name: param.keyword.trim().to_string(),
            description: param.description.trim().to_string(),
            required: param.required,
            r#type: kind_to_types(param.kind),
            default: param.default.map(pretty_value),
        })
        .collect();

    let examples: Vec<ExampleDoc> = func
        .examples()
        .iter()
        .map(|example| {
            let (r#return, raises) = match &example.result {
                Ok(result) => {
                    // Try to parse as JSON, otherwise treat as string
                    let value = serde_json::from_str(result)
                        .unwrap_or_else(|_| serde_json::Value::String(result.to_string()));
                    (Some(value), None)
                }
                Err(error) => (None, Some(error.to_string())),
            };

            let source = example.source.to_string();
            let title = example.title.to_string();
            ExampleDoc {
                title,
                source,
                r#return,
                raises,
            }
        })
        .collect();

    FunctionDoc {
        anchor: name.clone(),
        name,
        category: func.category().to_string(),
        description: trim_str(func.usage()),
        arguments,
        r#return: ReturnDoc {
            types: kind_to_types(func.return_kind()),
            rules: trim_slice(func.return_rules()),
        },
        internal_failure_reasons: trim_slice(func.internal_failure_reasons()),
        examples,
        notices: trim_slice(func.notices()),
        pure: func.pure(),
    }
}

fn kind_to_types(kind_bits: u16) -> Vec<String> {
    // All type bits combined
    if (kind_bits & kind::ANY) == kind::ANY {
        return vec!["any".to_string()];
    }

    let mut types = Vec::new();

    if (kind_bits & kind::BYTES) == kind::BYTES {
        types.push("string".to_string());
    }
    if (kind_bits & kind::INTEGER) == kind::INTEGER {
        types.push("integer".to_string());
    }
    if (kind_bits & kind::FLOAT) == kind::FLOAT {
        types.push("float".to_string());
    }
    if (kind_bits & kind::BOOLEAN) == kind::BOOLEAN {
        types.push("boolean".to_string());
    }
    if (kind_bits & kind::OBJECT) == kind::OBJECT {
        types.push("object".to_string());
    }
    if (kind_bits & kind::ARRAY) == kind::ARRAY {
        types.push("array".to_string());
    }
    if (kind_bits & kind::TIMESTAMP) == kind::TIMESTAMP {
        types.push("timestamp".to_string());
    }
    if (kind_bits & kind::REGEX) == kind::REGEX {
        types.push("regex".to_string());
    }
    if (kind_bits & kind::NULL) == kind::NULL {
        types.push("null".to_string());
    }

    assert!(!types.is_empty(), "kind_bits {kind_bits} produced no types");

    types
}

fn pretty_value(v: &Value) -> String {
    if let Value::Bytes(b) = v {
        str::from_utf8(&b)
            .map(String::from)
            .unwrap_or_else(|_| v.to_string())
    } else {
        v.to_string()
    }
}

fn trim_str(s: &'static str) -> String {
    s.trim().to_string()
}

fn trim_slice(slice: &'static [&'static str]) -> Vec<String> {
    slice.iter().map(|s| s.trim().to_string()).collect()
}
