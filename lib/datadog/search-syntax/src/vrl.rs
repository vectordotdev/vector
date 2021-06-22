use super::{
    field::{normalize_fields, Field},
    node::{BooleanType, Comparison, ComparisonValue},
};
use ordered_float::NotNan;
use vrl_parser::{
    ast::{self, Opcode},
    Span,
};

impl From<&BooleanType> for ast::Opcode {
    fn from(b: &BooleanType) -> Self {
        match b {
            BooleanType::And => ast::Opcode::And,
            BooleanType::Or => ast::Opcode::Or,
        }
    }
}

impl From<&Comparison> for ast::Opcode {
    fn from(c: &Comparison) -> Self {
        match c {
            Comparison::Gt => ast::Opcode::Gt,
            Comparison::Lt => ast::Opcode::Lt,
            Comparison::Gte => ast::Opcode::Ge,
            Comparison::Lte => ast::Opcode::Le,
        }
    }
}

impl From<ComparisonValue> for ast::Literal {
    fn from(cv: ComparisonValue) -> Self {
        match cv {
            ComparisonValue::String(value) => ast::Literal::String(value),
            ComparisonValue::Integer(value) => ast::Literal::Integer(value),
            ComparisonValue::Float(value) => {
                ast::Literal::Float(NotNan::new(value).expect("should be a float"))
            }
            ComparisonValue::Unbounded => panic!("unbounded values have no equivalent literal"),
        }
    }
}

/// Wrapper for a comparison value to be converted to a literal, with wrapped nodes.
impl From<ComparisonValue> for ast::Node<ast::Expr> {
    fn from(cv: ComparisonValue) -> Self {
        make_node(ast::Expr::Literal(make_node(cv.into())))
    }
}

/// Creates a VRL node with a default span.
pub fn make_node<T>(node: T) -> ast::Node<T> {
    ast::Node::new(Span::default(), node)
}

/// An `Expr::Op` from two expressions, and a separating operator.
pub fn make_op(expr1: ast::Node<ast::Expr>, op: Opcode, expr2: ast::Node<ast::Expr>) -> ast::Expr {
    ast::Expr::Op(make_node(ast::Op(
        Box::new(expr1),
        make_node(op),
        Box::new(expr2),
    )))
}

/// An `Expr::Query`, converting a string field to a lookup path.
pub fn make_queries<T: AsRef<str>>(field: T) -> Vec<(Field, ast::Expr)> {
    normalize_fields(field)
        .into_iter()
        .map(|field| {
            let query = ast::Expr::Query(make_node(ast::Query {
                target: make_node(ast::QueryTarget::External),
                path: make_node(
                    lookup::parser::parse_lookup(field.as_str())
                        .expect("should parse lookup")
                        .into(),
                ),
            }));

            (field, query)
        })
        .collect()
}

/// Makes a Regex string to be used with the `match` function for word boundary matching.
pub fn make_word_regex<T: AsRef<str>>(value: T) -> ast::Expr {
    ast::Expr::Literal(make_node(ast::Literal::Regex(format!(
        "\\b{}\\b",
        regex::escape(value.as_ref()).replace("\\*", ".*")
    ))))
}

/// Makes a Regex string to be used with the `match` function for arbitrary wildcard matching
pub fn make_wildcard_regex<T: AsRef<str>>(value: T) -> ast::Expr {
    ast::Expr::Literal(make_node(ast::Literal::Regex(format!(
        "^{}$",
        regex::escape(value.as_ref()).replace("\\*", ".*")
    ))))
}

/// Makes a string comparison expression.
pub fn make_string_comparison<T: AsRef<str>>(expr: ast::Expr, op: Opcode, value: T) -> ast::Expr {
    make_op(
        make_node(expr),
        op,
        make_node(ast::Expr::Literal(make_node(ast::Literal::String(
            String::from(value.as_ref()),
        )))),
    )
}

/// Makes a string literal.
pub fn make_string<T: AsRef<str>>(value: T) -> ast::Expr {
    ast::Expr::Literal(make_node(ast::Literal::String(value.as_ref().to_owned())))
}

/// Makes a boolean literal.
pub fn make_bool(value: bool) -> ast::Expr {
    ast::Expr::Literal(make_node(ast::Literal::Boolean(value)))
}

/// Makes a container group, for wrapping logic for easier negation.
pub(super) fn make_container_group(expr: ast::Expr) -> ast::Expr {
    ast::Expr::Container(make_node(ast::Container::Group(Box::new(make_node(
        ast::Group(make_node(expr)),
    )))))
}

/// Makes a negation wrapper for an inner expression.
pub fn make_not(expr: ast::Expr) -> ast::Expr {
    ast::Expr::Unary(make_node(ast::Unary::Not(make_node(ast::Not(
        make_node(()),
        Box::new(make_node(expr)),
    )))))
}

/// A `Expr::FunctionCall` based on a tag and arguments.
pub fn make_function_call<T: IntoIterator<Item = ast::Expr>>(tag: &str, arguments: T) -> ast::Expr {
    ast::Expr::FunctionCall(make_node(ast::FunctionCall {
        ident: make_node(ast::Ident::new(tag.to_string())),
        abort_on_error: false,
        arguments: arguments
            .into_iter()
            .map(|expr| {
                make_node(ast::FunctionArgument {
                    ident: None,
                    expr: make_node(expr),
                })
            })
            .collect(),
    }))
}

/// Makes a literal expression from something that converts to an `ast::Literal`.
pub fn make_literal<T: Into<ast::Literal>>(literal: T) -> ast::Expr {
    ast::Expr::Literal(make_node(literal.into()))
}

/// Makes a field expression that contains a runtime check on the field type where the field
/// type is a facet or a non-`tags` reserved field.
pub fn make_field_op<T: Into<ast::Literal> + std::fmt::Display + Clone>(
    field: Field,
    query: ast::Expr,
    op: ast::Opcode,
    value: T,
) -> ast::Expr {
    // Facets and non-`tags` reserved fields operate on numerals if the field type is float
    // or integer. Otherwise, they're treated as strings.
    match field {
        Field::Facet(f) | Field::Reserved(f) if f != "tags" => {
            // Check that the number is either an integer or a float.
            let num_check = make_container_group(make_op(
                make_node(make_function_call("is_integer", vec![query.clone()])),
                ast::Opcode::Or,
                make_node(make_function_call("is_float", vec![query.clone()])),
            ));

            // If we're dealing with a number, the range comparison should be numberic.
            let num_eq = make_op(
                make_node(query.clone()),
                op,
                make_node(make_literal(value.clone())),
            );

            // Final number expression, including int/float and range check.
            let num_expr = make_container_group(make_op(
                make_node(num_check),
                ast::Opcode::And,
                make_node(num_eq),
            ));

            // String comparison fallback.
            let string_expr = make_string_comparison(query, op, value.to_string());

            // Wire up the expressions, separated by `||`.
            recurse_op(vec![num_expr, string_expr].into_iter(), ast::Opcode::Or)
        }
        // If the field type doesn't support numeric operations, just compare by string.
        _ => make_string_comparison(query, op, value.to_string()),
    }
}

/// Recursive, nested expressions, ultimately returning a single `ast::Expr`.
pub fn recurse_op<I: ExactSizeIterator<Item = impl Into<ast::Expr>>, O: Into<ast::Opcode>>(
    mut exprs: I,
    op: O,
) -> ast::Expr {
    let expr = exprs.next().expect("must contain expression").into();
    let op = op.into();

    match exprs.len() {
        // If this is the last expression, just return it.
        0 => expr,
        // If there's one expression remaining, use it as the RHS; no need to wrap.
        1 => make_container_group(make_op(
            make_node(expr),
            op,
            make_node(exprs.next().expect("must contain expression").into()),
        )),
        // For 2+ expressions, recurse over the RHS, and wrap in a container group for atomicity.
        _ => make_container_group(make_op(
            make_node(expr),
            op,
            make_node(recurse_op(exprs, op)),
        )),
    }
}

/// Default recursion, using the `OR` operator.
pub fn recurse<I: ExactSizeIterator<Item = impl Into<ast::Expr>>>(exprs: I) -> ast::Expr {
    recurse_op(exprs, ast::Opcode::Or)
}

/// Coalesces an expression to <query> ?? false to avoid fallible states.
pub fn coalesce<T: Into<ast::Expr>>(expr: T) -> ast::Expr {
    make_container_group(make_op(
        make_node(expr.into()),
        Opcode::Err,
        make_node(ast::Expr::Literal(make_node(ast::Literal::Boolean(false)))),
    ))
}
