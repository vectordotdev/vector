use super::vrl::make_node;

use lazy_static::lazy_static;
use vrl_compiler::{Function, Result};
use vrl_parser::ast;
use vrl_stdlib as f;

/// Static express to parse Datadog tags to a VRL object.
static TAGS_QUERY: &str = r#".__datadog_tags = parse_key_value(join!(.tags, ","), field_delimiter: ",", key_value_delimiter: ":") ?? {}"#;

lazy_static! {
    static ref FUNCTIONS: Vec<Box<dyn Function>> = vec![
        Box::new(f::EndsWith),
        Box::new(f::Exists),
        Box::new(f::IsFloat),
        Box::new(f::IsInteger),
        Box::new(f::Includes),
        Box::new(f::Join),
        Box::new(f::Match),
        Box::new(f::ParseKeyValue),
        Box::new(f::StartsWith),
    ];
}

/// Compile an expression into a VRL program. This will include parsing of the `tags` field
/// which is make available on the `__datadog_tags` key.
pub fn compile<T: Into<ast::Expr>>(expr: T) -> Result {
    let mut program = vrl_parser::parse(TAGS_QUERY).expect("Datadog tags query should parse");

    let root = ast::RootExpr::Expr(make_node(expr.into()));
    program.0.push(make_node(root));

    vrl_compiler::compile(program, &FUNCTIONS)
}
