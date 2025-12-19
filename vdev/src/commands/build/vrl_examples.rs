use std::cell::LazyCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result, anyhow, ensure};
use clap::Args;
use itertools::Itertools;
use serde_json::{Value, json};
use vrl::compiler;
use vrl::compiler::function::ArgumentList;
use vrl::parser::ast::{self, RootExpr};
use vrl::prelude::{NotNan, Parameter};
use vrl::value;

use crate::app::{self, path};

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

/// Create bogus data just to get type information
fn args_from_kind(function_name: &str, p: &Parameter) -> Vec<vrl::value::Value> {
    const VALUE_EXCEPTIONS: LazyCell<
        HashMap<&'static str, HashMap<&'static str, Vec<vrl::value::Value>>>,
    > = LazyCell::new(|| {
        let proto_file_path = format!(
            "{}/lib/vector-vrl/tests/resources/protobuf_descriptor_set.desc",
            path()
        );
        HashMap::from([
            (
                "chunks",
                HashMap::from([("chunk_size", vec![vrl::value::Value::Integer(1)])]),
            ),
            (
                "decrypt",
                HashMap::from([("algorithm", vec![value!("AES-256-CFB")])]),
            ),
            (
                "del", // FIXME
                HashMap::from([("target", vec![value!(".field")])]),
            ),
            (
                "encode_proto",
                HashMap::from([
                    (
                        "desc_file",
                        vec![vrl::value::Value::Bytes(proto_file_path.clone().into())],
                    ),
                    ("message_type", vec![value!("test_protobuf.Person")]),
                ]),
            ),
            (
                "encrypt",
                HashMap::from([("algorithm", vec![value!("AES-256-CFB")])]),
            ),
            (
                "exists", // FIXME
                HashMap::from([("field", vec![value!(".field")])]),
            ),
            (
                "ip_cidr_contains",
                HashMap::from([("cidr", vec![value!("0.0.0.0/24")])]), // TODO add array
            ),
            (
                "parse_apache_log",
                HashMap::from([(
                    "format",
                    vec![value!("common"), value!("combined"), value!("error")],
                )]),
            ),
            (
                "parse_grok",
                HashMap::from([(
                    "pattern",
                    vec![value!(
                        "%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"
                    )],
                )]),
            ),
            (
                "parse_nginx_log",
                HashMap::from([(
                    "format",
                    vec![
                        value!("combined"),
                        value!("error"),
                        value!("ingress_upstreaminfo"),
                        value!("main"),
                    ],
                )]),
            ),
            (
                "parse_proto",
                HashMap::from([
                    (
                        "desc_file",
                        vec![vrl::value::Value::Bytes(proto_file_path.clone().into())],
                    ),
                    ("message_type", vec![value!("test_protobuf.Person")]),
                ]),
            ),
            (
                "random_int",
                HashMap::from([("max", vec![vrl::value::Value::Integer(1)])]),
            ),
            (
                "random_float",
                HashMap::from([(
                    "max",
                    vec![vrl::value::Value::Float(NotNan::try_from(1.0).unwrap())],
                )]),
            ),
        ])
    });

    if let Some(fn_override) = VALUE_EXCEPTIONS.get(function_name)
        && let Some(args_override) = fn_override.get(p.keyword)
    {
        return args_override.to_vec();
    }

    let kind = p.kind();
    let mut args = vec![];
    let any = kind.contains_undefined();

    if kind.contains_bytes() || any {
        args.push(vrl::value::Value::Bytes(Default::default()));
    }

    if kind.contains_integer() || any {
        args.push(vrl::value::Value::Integer(Default::default()));
    }

    if kind.contains_float() || any {
        args.push(vrl::value::Value::Float(Default::default()));
    }

    if kind.contains_boolean() || any {
        args.push(vrl::value::Value::Boolean(Default::default()));
    }

    if kind.contains_timestamp() || any {
        args.push(vrl::value::Value::Timestamp(Default::default()));
    }

    if kind.contains_regex() || any {
        args.push(vrl::value::Value::Regex(
            regex::Regex::from_str("").unwrap().into(),
        ));
    }

    if kind.contains_null() || any {
        args.push(vrl::value::Value::Null);
    }

    if kind.contains_array() || any {
        args.push(vrl::value::Value::Array(Default::default()));
    }

    if kind.contains_object() || any {
        args.push(vrl::value::Value::Object(Default::default()));
    }
    args
}

/// Verify a function works by compiling test VRL code and return its type
fn get_function_return_type(function_name: &str) -> Result<Option<String>> {
    let test_code = match function_name {
        "del" => "del(.test)",
        "exists" => "exists(.test)",
        "unnest" => ". = {\"events\": [1,2]}; . = unnest(.events)",
        // Filter takes 2 closure parameters: key/index and value
        "filter" => "filter([1,2]) -> |_index, _value| { true }",
        // for_each takes 2 closure parameters: key/index and value
        "for_each" => "for_each([1,2]) -> |_index, _value| { . = _value }",
        // map_keys takes 1 closure parameter: key
        "map_keys" => "map_keys({\"a\": 1}) -> |key| { key }",
        // map_values takes 1 closure parameter: value
        "map_values" => "map_values({\"a\": 1}) -> |value| { value }",
        // replace_with takes 1 closure parameter: match object
        "replace_with" => "replace_with(\"test\", r'test') -> |_match| { \"x\" }",
        _ => return Ok(None),
    };

    let fns = vrl::stdlib::all();
    let result = compiler::compile(test_code, &fns)
        .map_err(|diagnostic_list| anyhow!("{diagnostic_list:?}"))
        .with_context(|| {
            format!("Failed to compile test code for {function_name}\nCode:\n{test_code}")
        })?;
    let type_info = result.program.final_type_info();
    Ok(Some(type_info.result.to_string()))
}

/// Test result for special functions
enum TestResult {
    /// Return ArgumentList for normal testing
    ArgumentLists(Vec<ArgumentList>),
    /// Return types derived from compiling actual VRL code
    DerivedTypes(Vec<String>),
}

/// Recursively find all function calls in an expression that match the given function name
fn find_matching_function_calls(
    expr: &ast::Expr,
    function_name: &str,
    source: &str,
    return_types: &mut HashSet<String>,
) {
    match expr {
        ast::Expr::FunctionCall(fc_node) => {
            let fc = fc_node.inner();
            // Check if this function call matches our target functionIfS
            if fc.ident.inner().to_string() == function_name {
                // Extract the source code for this function call using its span
                let span = fc_node.span();
                let function_source = &source[span.start()..span.end()];

                // Compile just this function call
                if let Ok(compiled) = vrl::compiler::compile(function_source, &vrl::stdlib::all()) {
                    let type_info = compiled.program.final_type_info();
                    let result = type_info.result;

                    if result.is_object() || result.is_array() {
                        if result.is_object() {
                            return_types.insert("object".to_owned());
                        }
                        if result.is_array() {
                            return_types.insert("array".to_owned());
                        }
                    } else {
                        return_types.insert(result.to_string());
                    }
                }
            }

            // Recursively check arguments
            for arg in &fc.arguments {
                find_matching_function_calls(
                    arg.inner().expr.inner(),
                    function_name,
                    source,
                    return_types,
                );
            }

            // Check closure if present
            if let Some(closure) = &fc.closure {
                // Block is a tuple struct: Block(pub Vec<Node<Expr>>)
                for expr_node in &closure.inner().block.inner().0 {
                    find_matching_function_calls(
                        expr_node.inner(),
                        function_name,
                        source,
                        return_types,
                    );
                }
            }
        }
        ast::Expr::Op(op_node) => {
            let op = op_node.inner();
            // Op is a tuple: (Box<Node<Expr>>, Node<Opcode>, Box<Node<Expr>>)
            find_matching_function_calls(op.0.inner(), function_name, source, return_types);
            find_matching_function_calls(op.2.inner(), function_name, source, return_types);
        }
        ast::Expr::Assignment(assign_node) => {
            let assign = assign_node.inner();
            match assign {
                ast::Assignment::Single { expr, .. } => {
                    find_matching_function_calls(expr.inner(), function_name, source, return_types);
                }
                ast::Assignment::Infallible { expr, .. } => {
                    find_matching_function_calls(expr.inner(), function_name, source, return_types);
                }
            }
        }
        ast::Expr::IfStatement(if_stmt_node) => {
            let if_stmt = if_stmt_node.inner();
            // Check predicate expression
            if let ast::Predicate::One(pred_expr) = if_stmt.predicate.inner() {
                find_matching_function_calls(
                    pred_expr.inner(),
                    function_name,
                    source,
                    return_types,
                );
            }

            // Check if block - Block is a tuple struct: Block(pub Vec<Node<Expr>>)
            for expr_node in &if_stmt.if_node.inner().0 {
                find_matching_function_calls(
                    expr_node.inner(),
                    function_name,
                    source,
                    return_types,
                );
            }

            // Check else block
            if let Some(else_block) = &if_stmt.else_node {
                for expr_node in &else_block.inner().0 {
                    find_matching_function_calls(
                        expr_node.inner(),
                        function_name,
                        source,
                        return_types,
                    );
                }
            }
        }
        ast::Expr::Container(container_node) => {
            let container = container_node.inner();
            match container {
                ast::Container::Array(_) | ast::Container::Object(_) => {
                    // Array and Object fields are private, skip for now
                    // The function call will still be captured if it's at the top level
                }
                ast::Container::Block(block_node) => {
                    // Block is a tuple struct: Block(pub Vec<Node<Expr>>)
                    for expr_node in &block_node.inner().0 {
                        find_matching_function_calls(
                            expr_node.inner(),
                            function_name,
                            source,
                            return_types,
                        );
                    }
                }
                ast::Container::Group(group_node) => {
                    // Group is a tuple struct: Group(pub Node<Expr>)
                    find_matching_function_calls(
                        group_node.inner().0.inner(),
                        function_name,
                        source,
                        return_types,
                    );
                }
            }
        }
        ast::Expr::Unary(_) => {
            // Unary::Not fields are private, can't access nested expr
            // Skip for now - function calls in unary are rare
        }
        ast::Expr::Query(query_node) => {
            let query = query_node.inner();
            // QueryTarget can contain a FunctionCall or Container
            // Process them by checking if they match our function
            match query.target.inner() {
                ast::QueryTarget::FunctionCall(fc) => {
                    // Check if this function call matches our target function
                    if fc.ident.inner().to_string() == function_name {
                        // Extract the source code for the entire query using its span
                        let span = query_node.span();
                        let function_source = &source[span.start()..span.end()];

                        // Compile just this query expression
                        if let Ok(compiled) =
                            vrl::compiler::compile(function_source, &vrl::stdlib::all())
                        {
                            let type_info = compiled.program.final_type_info();
                            return_types.insert(type_info.result.to_string());
                        }
                    }
                }
                ast::QueryTarget::Container(container) => {
                    // Recursively check container expressions
                    match container {
                        ast::Container::Block(block_node) => {
                            for expr_node in &block_node.inner().0 {
                                find_matching_function_calls(
                                    expr_node.inner(),
                                    function_name,
                                    source,
                                    return_types,
                                );
                            }
                        }
                        ast::Container::Group(group_node) => {
                            find_matching_function_calls(
                                group_node.inner().0.inner(),
                                function_name,
                                source,
                                return_types,
                            );
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        ast::Expr::Abort(abort_node) => {
            let abort = abort_node.inner();
            if let Some(message) = &abort.message {
                find_matching_function_calls(message.inner(), function_name, source, return_types);
            }
        }
        ast::Expr::Return(ret_node) => {
            let ret = ret_node.inner();
            find_matching_function_calls(ret.expr.inner(), function_name, source, return_types);
        }
        ast::Expr::Literal(_) | ast::Expr::Variable(_) => {
            // Base cases - no nested expressions
        }
    }
}

/// Create arguments from parameter specification or derive types for special functions
fn create_arguments_for_function(function_name: &str, params: &[Parameter]) -> Result<TestResult> {
    // For query/closure functions, compile actual VRL code to derive return types
    if let Some(return_type) = get_function_return_type(function_name)? {
        return Ok(TestResult::DerivedTypes(vec![return_type]));
    }

    // Default: regular parameters
    let required = params.iter().filter(|p| p.required).collect_vec();

    let required_args = required
        .iter()
        .map(|p| std::iter::once(p.keyword).cartesian_product(args_from_kind(function_name, p)))
        .multi_cartesian_product();

    let arg_lists = required_args
        .map(|args| {
            let arguments: HashMap<&'static str, vrl::value::Value> =
                HashMap::from_iter(args.into_iter());
            arguments.into()
        })
        .collect();

    Ok(TestResult::ArgumentLists(arg_lists))
}

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

            let documented_return = function_obj
                .get("return")
                .and_then(|r| r.get("types"))
                .with_context(|| panic!("{function_name} doesn't have return"))?
                .as_array()
                .with_context(|| panic!("{function_name}.return isn't an array"))?;
            let mut documented_return = documented_return
                .into_iter()
                .map(|v| v.as_str().unwrap())
                .collect_vec();
            documented_return.sort();

            let actual_return = {
                let mut return_type = HashSet::new();

                // Create argument lists or derive types from VRL compilation
                match create_arguments_for_function(function_name, function.parameters())? {
                    TestResult::ArgumentLists(arg_lists) => {
                        // Normal testing with ArgumentList
                        for arguments in arg_lists {
                            let ctx = &mut vrl::compiler::function::FunctionCompileContext::new(
                                Default::default(),
                                Default::default(),
                            );

                            let compiled = function
                                .compile(&Default::default(), ctx, arguments)
                                .unwrap_or_else(|e| {
                                    panic!("function `{function_name}` failed to compile: {e:?}")
                                });
                            let kind = compiled.type_def(&Default::default());
                            if kind.is_object() || kind.is_array() {
                                if kind.is_object() {
                                    return_type.insert("object".to_owned());
                                }
                                if kind.is_array() {
                                    return_type.insert("array".to_owned());
                                }
                            } else {
                                return_type.insert(kind.to_string());
                            }
                        }
                    }
                    TestResult::DerivedTypes(types) => {
                        // Query/closure functions: types derived from VRL compilation
                        for t in types {
                            return_type.insert(t);
                        }
                    }
                }

                for example in function.examples() {
                    if function.identifier() == "encode_proto"
                        || function.identifier() == "parse_proto"
                    {
                        continue;
                    }
                    let ast = vrl::parser::parse(example.source).expect("Invalid VRL AST");
                    let root_exprs = ast.0;

                    // Walk the AST to find all function calls matching the current function
                    for root_expr in &root_exprs {
                        if let RootExpr::Expr(expr_node) = &root_expr.inner() {
                            find_matching_function_calls(
                                &expr_node.inner(),
                                function.identifier(),
                                example.source,
                                &mut return_type,
                            );
                        }
                    }
                }

                let mut return_type = return_type.into_iter().collect_vec();
                return_type.sort();
                return_type
            };

            if documented_return != actual_return {
                println!("{function_name:?}");
                println!("{documented_return:?} != {actual_return:?}");
            }

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
