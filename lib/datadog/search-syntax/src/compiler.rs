use super::vrl::make_node;
use lazy_static::lazy_static;
use vrl_compiler::{Function, Result};
use vrl_parser::ast;
use vrl_stdlib as f;

static TAGS_QUERY: &str = r#".__datadog_tags = parse_key_value(join!(.tags, ","), field_delimiter: ",", key_value_delimiter: ":") ?? {}"#;

lazy_static! {
    static ref FUNCTIONS: Vec<Box<dyn Function>> = vec![
        Box::new(f::Exists),
        Box::new(f::Join),
        Box::new(f::Match),
        Box::new(f::ParseKeyValue)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse, Builder};

    #[test]
    /// Test that the program compiles, and has the right number of expressions.
    fn compiles() {
        let node = parse("a:[1 TO 5]").unwrap();
        let builder = Builder::new();
        let program = compile(builder.build(&node)).unwrap();

        assert!(program.into_iter().len() == 2);
    }
}
