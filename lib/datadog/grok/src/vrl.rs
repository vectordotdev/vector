use std::sync::Arc;

use grok::Grok;

use ::vrl::{
    diagnostic::{Label, Span},
    prelude::*,
    state::Compiler,
};
use lookup::LookupBuf;
use shared::btreemap;
use vrl_compiler::{compile, Program};
use vrl_parser::ast as vrl_ast;
use vrl_parser::ast::{AssignmentTarget, Opcode, RootExpr};

use crate::ast as grok_ast;
use crate::ast::FunctionArgument;
use crate::parse_grok::Error as GrokError;
use crate::parse_grok::{parse_grok_rules, GrokRule};
use crate::vrl_helpers::*;

/**
    Compiles a list of DataDog grok rules to a VRL program as:

    // check the first pattern
    if parsed == null {
        parsed = parse_datadog_grok(.message, pattern1)
        if err == null { // it matched - apply filters
            parsed.http.status_code = to_int(parsed.http.status_code)
            parsed.http.url_details = parse_url(parsed.http.url)
        }
    }
    // check the second, third pattern and so on
    if parsed == null {
        parsed = parse_datadog_grok(.message, pattern2)
        if err == null { // it matched - apply filters
            ...
        }
    }
    ...
    // merge the parsed result to the source event
    .custom = merge(., parsed)
*/
pub fn compile_to_vrl(
    source_field: Option<String>,
    support_rules: &Vec<String>,
    match_rules: &Vec<String>,
) -> vrl_compiler::Result {
    let target_var_name = "parsed";
    let err_var_name = "err";
    let source_field = source_field.unwrap_or("message".to_string());
    let grok_rule_exprs = parse_grok_rules(&support_rules, &match_rules)
        .map_err(|e| {
            vec![Box::new(Error::FailedToParse(format!("{}", e))) as Box<dyn DiagnosticError>]
        })?
        .iter()
        .map(|rule| grok_rule_to_expr(target_var_name, err_var_name, source_field.as_str(), rule))
        .collect::<std::result::Result<Vec<vrl_ast::Expr>, GrokError>>()
        .map_err(|e| {
            vec![Box::new(Error::FailedToParse(format!("{}", e))) as Box<dyn DiagnosticError>]
        })?;

    let mut exprs = vec![];
    exprs.extend(grok_rule_exprs);
    // . = merge(., target_var_name)
    let merge = vrl_ast::Expr::Assignment(make_node(vrl_ast::Assignment::Single {
        target: make_node(AssignmentTarget::External(Some(".custom".parse().unwrap()))),
        op: vrl_ast::AssignmentOp::Assign,
        expr: Box::new(make_assignment_expr_node(make_function_call(
            "merge",
            vec![
                vrl_ast::Expr::Query(make_node(vrl_ast::Query {
                    target: make_node(vrl_ast::QueryTarget::External),
                    path: make_node(LookupBuf::root()),
                })),
                make_variable(target_var_name),
                vrl_ast::Expr::Literal(make_node(vrl_ast::Literal::Boolean(true))),
            ],
            false,
        ))),
    }));
    exprs.push(merge);
    let program = vrl_ast::Program(vec![make_node(RootExpr::Expr(make_node(make_block(
        exprs,
    ))))]);
    let mut vrl_functions: Vec<Box<dyn vrl_compiler::function::Function>> = vec![];
    // register all VRL public functions
    vrl_functions.extend(vrl_stdlib::all());
    // add "private" functions
    vrl_functions.push(Box::new(ParseDataDogGrok));

    compile(program, &vrl_functions)
}

/**
    Converts a grok rule to a corresponding VRL expr:
    if parsed == null {
        parsed, err = parse_datadog_grok(value, pattern1)
        if err == nil { { // it matched - apply filters
            target_var.http.status_code = to_int(target_var.http.status_code)
            target_var.http.url_details = parse_url(target_var.http.url)
        }
    }
*/
fn grok_rule_to_expr(
    target_var_name: &str,
    err_var_name: &str,
    source_field: &str,
    grok_rule: &GrokRule,
) -> std::result::Result<vrl_ast::Expr, GrokError> {
    let source_query = make_query(source_field);
    let parse_grok = make_infallible_assignment(
        target_var_name,
        err_var_name,
        make_function_call(
            "parse_datadog_grok",
            vec![
                source_query,
                vrl_ast::Expr::Literal(make_node(vrl_ast::Literal::String(
                    grok_rule.pattern.clone(),
                ))),
            ],
            false,
        ),
    );

    let filters = grok_rule
        .filters
        .iter()
        .map(|(path, filters)| {
            filters
                .iter()
                .map(|filter| {
                    let filter_call = make_filter_call(
                        make_internal_query(target_var_name, path.clone()),
                        filter,
                    )?;
                    Ok(vrl_ast::Expr::Assignment(make_node(
                        vrl_ast::Assignment::Single {
                            target: make_node(vrl_ast::AssignmentTarget::Internal(
                                vrl_ast::Ident::new(target_var_name.to_string()),
                                Some(path.clone()),
                            )),
                            op: vrl_ast::AssignmentOp::Assign,
                            expr: Box::new(make_assignment_expr_node(filter_call)),
                        },
                    )))
                })
                .collect::<std::result::Result<Vec<vrl_ast::Expr>, GrokError>>()
        })
        .flat_map(|result| match result {
            // this is a trick to flatten exprs
            Ok(vec) => vec.into_iter().map(|item| Ok(item)).collect(),
            Err(er) => vec![Err(er)],
        })
        .collect::<std::result::Result<Vec<vrl_ast::Expr>, GrokError>>()?;

    let parsed_eq_null = make_op(
        make_node(make_variable(target_var_name)),
        Opcode::Eq,
        make_node(make_null()),
    );
    let er_eq_null = make_op(
        make_node(make_variable(err_var_name)),
        Opcode::Eq,
        make_node(make_null()),
    );
    let apply_filters_if_parsed = make_if(er_eq_null, make_block(filters));
    let parse_if_not_parsed = make_if(
        parsed_eq_null,
        make_block(vec![parse_grok, apply_filters_if_parsed]),
    );

    Ok(parse_if_not_parsed)
}

fn make_filter_call(
    value: vrl_ast::Expr,
    filter: &grok_ast::Function,
) -> std::result::Result<vrl_ast::Expr, GrokError> {
    match filter.name.as_ref() {
        "integer" => Ok(make_coalesce(
            make_function_call("to_int", vec![value], false),
            make_null(),
        )),
        "integerExt" => Ok(
            /// scientific notation is supported by float conversion,
            /// so first convert it to float and then to int
            make_coalesce(
                make_function_call(
                    "to_int",
                    vec![make_coalesce(
                        make_function_call("to_float", vec![value], false),
                        vrl_ast::Expr::Literal(make_node(
                            vrl_ast::Literal::String("not a valid integer".into()).into(),
                        )),
                    )],
                    false,
                ),
                make_null(),
            ),
        ),
        "number" => Ok(make_coalesce(
            make_function_call("to_float", vec![value], false),
            make_null(),
        )),
        "numberExt" => Ok(make_coalesce(
            make_function_call("to_float", vec![value], false),
            make_null(),
        )),
        "boolean" => {
            if filter.args.is_some() && !filter.args.as_ref().unwrap().is_empty() {
                if let FunctionArgument::ARG(ref true_pattern) = filter.args.as_ref().unwrap()[0] {
                    return Ok(make_if_else(
                        make_function_call(
                            "match",
                            vec![
                                value,
                                vrl_ast::Expr::Literal(make_node(vrl_ast::Literal::Regex(
                                    format!(
                                        "^{}$",
                                        true_pattern.try_bytes_utf8_lossy().map_err(|_| {
                                            GrokError::InvalidFunctionArguments(filter.name.clone())
                                        })?
                                    ),
                                ))),
                            ],
                            false,
                        ),
                        vrl_ast::Expr::Literal(make_node(vrl_ast::Literal::Boolean(true))),
                        vrl_ast::Expr::Literal(make_node(vrl_ast::Literal::Boolean(false))),
                    ));
                }
            }
            Ok(make_coalesce(
                make_function_call("to_bool", vec![value], false),
                make_null(),
            ))
        }
        "nullIf" => {
            if filter.args.is_some() && !filter.args.as_ref().unwrap().is_empty() {
                if let FunctionArgument::ARG(ref null_value) = filter.args.as_ref().unwrap()[0] {
                    return Ok(make_if_else(
                        make_op(
                            make_node(value.clone()),
                            Opcode::Eq,
                            make_node(vrl_ast::Expr::Literal(make_node(vrl_ast::Literal::String(
                                format!(
                                    "{}",
                                    null_value.try_bytes_utf8_lossy().map_err(|_| {
                                        GrokError::InvalidFunctionArguments(filter.name.clone())
                                    })?
                                ),
                            )))),
                        ),
                        make_null(),
                        value,
                    ));
                }
            }
            Err(GrokError::InvalidFunctionArguments(filter.name.clone()))
        }
        "scale" => {
            if filter.args.is_some() && !filter.args.as_ref().unwrap().is_empty() {
                if let FunctionArgument::ARG(Value::Integer(scale_factor)) =
                    filter.args.as_ref().unwrap()[0]
                {
                    // VRL supports string multiplication - we don't want that
                    let is_number = make_op(
                        make_node(make_function_call("is_float", vec![value.clone()], false)),
                        Opcode::Or,
                        make_node(make_function_call("is_integer", vec![value.clone()], false)),
                    );
                    // if is_float(value) || is_integer(value) { value * scale_factor} else { null } ?? null
                    return Ok(make_coalesce(
                        make_if_else(
                            is_number,
                            make_op(
                                make_node(value),
                                Opcode::Mul,
                                make_node(vrl_ast::Expr::Literal(make_node(
                                    vrl_ast::Literal::Integer(scale_factor),
                                ))),
                            ),
                            make_null(),
                        ),
                        make_null(),
                    ));
                }
            }
            Err(GrokError::InvalidFunctionArguments(filter.name.clone()))
        }
        "json" => Ok(make_coalesce(
            make_function_call("parse_json", vec![value], false),
            make_null(),
        )),
        "rubyhash" => Ok(make_coalesce(
            make_function_call("parse_ruby_hash", vec![value], false),
            make_null(),
        )),
        "querystring" => Ok(make_coalesce(
            make_function_call("parse_query_string", vec![value], false),
            make_null(),
        )),
        "lowercase" => Ok(make_coalesce(
            make_function_call("downcase", vec![value], false),
            make_null(),
        )),
        "uppercase" => Ok(make_coalesce(
            make_function_call("upcase", vec![value], false),
            make_null(),
        )),
        _ => Err(GrokError::UnsupportedFilter(filter.name.clone())),
    }
}

/// a "private" VRL function which is similar to `parse_grok`, but also supports DD grok patterns
/// and treats field names as lookups
#[derive(Clone, Copy, Debug)]
struct ParseDataDogGrok;

impl Function for ParseDataDogGrok {
    fn identifier(&self) -> &'static str {
        "parse_datadog_grok"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "pattern",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        unimplemented!("it is a private function")
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        let pattern = arguments
            .required_literal("pattern")?
            .to_value()
            .try_bytes_utf8_lossy()
            .expect("grok pattern not bytes")
            .into_owned();

        let mut grok = initialize_grok();
        let pattern = Arc::new(
            grok.compile(&pattern, true)
                .map_err(|e| Box::new(Error::InvalidGrokPattern(e)) as Box<dyn DiagnosticError>)?,
        );

        Ok(Box::new(ParseDataDogGrokRuleFn { value, pattern }))
    }
}

include!(concat!(env!("OUT_DIR"), "/patterns.rs"));
fn initialize_grok() -> Grok {
    let mut grok = grok::Grok::with_patterns();

    // insert DataDog grok patterns
    for &(key, value) in PATTERNS {
        grok.insert_definition(String::from(key), String::from(value));
    }
    grok
}

#[derive(Debug)]
pub enum Error {
    InvalidGrokPattern(grok::Error),
    FailedToParse(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidGrokPattern(err) => write!(f, "{}", err.to_string()),
            Error::FailedToParse(err) => write!(f, "{}", err),
        }
    }
}

impl std::error::Error for Error {}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        0
    }

    fn labels(&self) -> Vec<Label> {
        match self {
            Error::InvalidGrokPattern(err) => {
                vec![Label::primary(
                    format!("grok pattern error: {}", err.to_string()),
                    Span::default(),
                )]
            }
            Error::FailedToParse(_) => vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParseDataDogGrokRuleFn {
    value: Box<dyn Expression>,
    pattern: Arc<grok::Pattern>,
}

impl Expression for ParseDataDogGrokRuleFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let bytes = value.try_bytes_utf8_lossy()?;

        let mut result = value!(btreemap! {});

        if let Some(ref matches) = self.pattern.match_against(&bytes) {
            for (name, value) in matches.iter() {
                let path = name.parse().expect("path always should be valid");
                result.insert(&path, value!(value));
            }
            return Ok(result);
        };

        Err("unable to parse input with grok pattern".into())
    }

    fn type_def(&self, _: &Compiler) -> TypeDef {
        TypeDef::new()
            .fallible()
            .add_object::<(), Kind>(map! { (): Kind::all() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compiles() {
        let grok = initialize_grok();
        compile_to_vrl(
            None,
            // support rules
            vec![
                r#"_auth %{notSpace:http.auth:nullIf("-")}"#.to_string(),
                r#"_bytes_written %{NUMBER:network.bytes_written}"#.to_string(),
                r#"_client_ip %{ipOrHost:network.client.ip}"#.to_string(),
                r#"_version HTTP\/(?<http.version>\d+\.\d+)"#.to_string(),
                r#"_url %{notSpace:http.url}"#.to_string(),
                r#"_ident %{notSpace:http.ident}"#.to_string(),
                r#"_user_agent %{regex("[^\\\"]*"):http.useragent}"#.to_string(),
                r#"_referer %{notSpace:http.referer}"#.to_string(),
                r#"_status_code %{NUMBER:http.status_code:integer}"#.to_string(),
                r#"_method %{word:http.method}"#.to_string(),
                r#"_date_access %{date("dd/MMM/yyyy:HH:mm:ss Z"):date_access}"#.to_string(),
                r#"_x_forwarded_for %{regex("[^\\\"]*"):http._x_forwarded_for:nullIf("-")}"#.to_string()].as_ref(),
            // match rules
            vec![
                r#"access.common %{_client_ip} %{_ident} %{_auth} \[%{_date_access}\] "(?>%{_method} |)%{_url}(?> %{_version}|)" %{_status_code} (?>%{_bytes_written}|-)"#.to_string(),
            ].as_ref());
    }
}
