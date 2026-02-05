use anyhow::Result;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use vrl::compiler::Function;
use vrl::compiler::value::kind;

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
    internal_failure_reasons: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    examples: Vec<ExampleDoc>,
    deprecated: bool,
    pure: bool,
}

#[derive(Serialize)]
struct ArgumentDoc {
    name: String,
    description: String,
    required: bool,
    r#type: Vec<String>,
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
    let category = infer_category(&name);

    let arguments: Vec<ArgumentDoc> = func
        .parameters()
        .iter()
        .map(|param| ArgumentDoc {
            name: param.keyword.to_string(),
            description: param.description.to_string(),
            required: param.required,
            r#type: kind_to_types(param.kind),
        })
        .collect();

    let examples: Vec<ExampleDoc> = func
        .examples()
        .iter()
        .map(|example| {
            let (return_value, raises) = match &example.result {
                Ok(result) => {
                    // Try to parse as JSON, otherwise treat as string
                    let value = serde_json::from_str(result)
                        .unwrap_or_else(|_| serde_json::Value::String(result.to_string()));
                    (Some(value), None)
                }
                Err(error) => (None, Some(error.to_string())),
            };
            ExampleDoc {
                title: example.title.to_string(),
                source: example.source.to_string(),
                r#return: return_value,
                raises,
            }
        })
        .collect();

    FunctionDoc {
        anchor: name.clone(),
        name,
        category: category.to_string(),
        description: func.usage().to_string(),
        arguments,
        r#return: ReturnDoc {
            types: vec!["any".to_string()], // Stub - could derive from TypeDef later
            rules: vec![],
        },
        internal_failure_reasons: vec![], // Stub
        examples,
        deprecated: false, // Stub
        pure: true,        // Stub - default true
    }
}

fn kind_to_types(kind_bits: u16) -> Vec<String> {
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

    if types.is_empty() {
        types.push("any".to_string());
    }

    types
}

fn infer_category(name: &str) -> &'static str {
    match name {
        // Exact matches first (before patterns that might match them)

        // Debug functions (log is a debug function, not number)
        "log" | "assert" | "assert_eq" | "abort" => "Debug",

        // Timestamp - exact matches before patterns
        "now" | "from_unix_timestamp" | "format_timestamp" => "Timestamp",

        // Cryptography - exact matches
        "encrypt" | "decrypt" | "md5" => "Cryptography",

        // String functions - exact matches (including case conversion variants)
        "upcase" | "downcase" | "camelcase" | "snakecase" | "kebabcase" => "String",
        "screaming_snakecase" | "screamingsnakecase" | "pascalcase" => "String",
        "capitalize" | "strip_whitespace" | "truncate" | "trim" => "String",
        "strip_ansi_escape_codes" | "starts_with" | "ends_with" | "contains" | "contains_all" => {
            "String"
        }
        "slice" | "split" | "join" | "replace" | "replace_with" => "String",
        "redact" | "find" | "substring" | "strlen" | "sieve" => "String",
        "match_datadog_query" => "String",

        // Array functions - exact matches (reverse is Array, not String)
        "append" | "push" | "pop" | "shift" | "unshift" => "Array",
        "flatten" | "chunks" | "unique" | "includes" | "reverse" => "Array",
        "tally" | "tally_value" | "unnest" => "Array",

        // Object functions
        "keys" | "values" | "object" | "merge" | "compact" => "Object",
        "remove" | "set" | "get" => "Object",
        "object_from_array" | "unflatten" => "Object",

        // Number functions
        "abs" | "ceil" | "floor" | "round" | "mod" => "Number",
        "int" | "float" | "haversine" => "Number",
        "format_int" | "format_number" => "Number",

        // System functions
        "get_env_var" | "get_hostname" | "get_timezone_name" => "System",
        "http_request" => "System",

        // Secret/Event functions
        "get_secret" | "set_secret" | "remove_secret" => "Event",

        // Enumerate functions
        "for_each" | "filter" | "map_keys" | "map_values" => "Enumerate",

        // Checksum functions
        "crc" | "seahash" | "xxhash" => "Checksum",

        // Coerce by name
        "bool" | "string" | "array" => "Coerce",

        // Path functions
        "exists" | "path_matches" | "length" => "Path",
        "basename" | "dirname" | "split_path" => "Path",

        // Convert functions
        "type_def" | "typeof" | "set_semantic_meaning" => "Convert",
        "tag_types_externally" => "Convert",

        // IP functions
        "community_id" | "dns_lookup" | "reverse_dns" => "IP",

        // Random/UUID functions
        "uuid_from_friendly_id" => "Random",

        // Type/validation functions
        "validate_json_schema" => "Type",

        // Now the pattern matches

        // Parse functions
        n if n.starts_with("parse_") => "Parse",

        // Codec functions
        n if n.starts_with("encode_") => "Codec",
        n if n.starts_with("decode_") => "Codec",

        // Type checking functions
        n if n.starts_with("is_") => "Type",

        // Coerce functions
        n if n.starts_with("to_") => "Coerce",

        // IP functions
        n if n.contains("ip") || n.contains("cidr") => "IP",

        // Timestamp functions
        n if n.contains("timestamp") => "Timestamp",

        // Cryptography functions
        n if n.starts_with("sha") || n.contains("hmac") => "Cryptography",

        // String matching functions
        n if n.starts_with("match") => "String",

        // Object functions with del prefix
        n if n.starts_with("del") => "Object",

        // Enrichment functions
        n if n.starts_with("get_enrichment_table_record")
            || n.starts_with("find_enrichment_table") =>
        {
            "Enrichment"
        }

        // Random functions
        n if n.starts_with("random") || n.starts_with("uuid") => "Random",

        // Metrics
        n if n.contains("metric") => "Metrics",

        // Default to String as a reasonable fallback (most new functions are string manipulation)
        _ => "String",
    }
}
