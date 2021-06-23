use lookup::LookupBuf;
use vrl_parser::{
    ast::{self, AssignmentTarget, Expr, Node, Opcode},
    Span,
};

/// TODO this is mostly copied(added methods are without comments) from DD search syntax, move it to a shared lib.

/// Creates a VRL node with a default span.
pub fn make_node<T>(node: T) -> ast::Node<T> {
    ast::Node::new(Span::default(), node)
}

/// Creates a VRL node with a span (1, 1) to avoid subtracting with overflow in the assignment  
pub fn make_assignment_expr_node<T>(node: T) -> ast::Node<T> {
    ast::Node::new(Span::new(1, 1), node)
}

/// An `Expr::Op` from two expressions, and a separating operator.
pub fn make_op(expr1: ast::Node<ast::Expr>, op: Opcode, expr2: ast::Node<ast::Expr>) -> ast::Expr {
    ast::Expr::Op(make_node(ast::Op(
        Box::new(expr1),
        make_node(op),
        Box::new(expr2),
    )))
}

pub fn make_internal_query(target: &str, path: LookupBuf) -> Expr {
    ast::Expr::Query(make_node(ast::Query {
        target: make_node(ast::QueryTarget::Internal(ast::Ident::new(target))),
        path: make_node(path),
    }))
}

pub fn make_query(field_name: &str) -> Expr {
    ast::Expr::Query(make_node(ast::Query {
        target: make_node(ast::QueryTarget::External),
        path: make_node(
            lookup::parser::parse_lookup(field_name)
                .expect("should parse lookup")
                .into(),
        ),
    }))
}

/// Makes a Regex string to be used with the `match`.
fn make_regex<T: AsRef<str>>(value: T) -> ast::Expr {
    ast::Expr::Literal(make_node(ast::Literal::Regex(format!(
        "\\b{}\\b",
        regex::escape(value.as_ref()).replace("\\*", ".*")
    ))))
}

/// Makes a string comparison expression.
fn make_string_comparison<T: AsRef<str>>(expr: ast::Expr, op: Opcode, value: T) -> ast::Expr {
    make_op(
        make_node(expr),
        op,
        make_node(ast::Expr::Literal(make_node(ast::Literal::String(
            String::from(value.as_ref()),
        )))),
    )
}

/// Makes a container group, for wrapping logic for easier negation.
pub fn make_container_group(expr: ast::Expr) -> ast::Expr {
    ast::Expr::Container(make_node(ast::Container::Group(Box::new(make_node(
        ast::Group(make_node(expr)),
    )))))
}

pub fn make_variable(name: &str) -> ast::Expr {
    ast::Expr::Variable(make_node(ast::Ident::new(name.to_string())))
}

pub fn make_null() -> ast::Expr {
    ast::Expr::Literal(make_node(ast::Literal::Null))
}

pub fn make_assignment_target(name: &str) -> Node<AssignmentTarget> {
    make_node(ast::AssignmentTarget::Internal(
        ast::Ident::new(name.to_string()),
        None,
    ))
}

pub fn make_single_assignment(ok_target: &str, expr: ast::Expr) -> ast::Expr {
    ast::Expr::Assignment(make_node(ast::Assignment::Single {
        target: make_assignment_target(ok_target),
        op: ast::AssignmentOp::Assign,
        expr: Box::new(make_assignment_expr_node(expr)),
    }))
}

pub fn make_infallible_assignment(ok_target: &str, err_target: &str, expr: ast::Expr) -> ast::Expr {
    ast::Expr::Assignment(make_node(ast::Assignment::Infallible {
        ok: make_assignment_target(ok_target),
        err: make_assignment_target(err_target),
        op: ast::AssignmentOp::Assign,
        expr: Box::new(make_assignment_expr_node(expr)),
    }))
}

pub fn make_if(predicate: ast::Expr, consequent: ast::Expr) -> ast::Expr {
    ast::Expr::IfStatement(make_node(ast::IfStatement {
        predicate: make_node(ast::Predicate::One(Box::new(make_node(predicate)))),
        consequent: make_node(ast::Block(vec![make_node(consequent)])),
        alternative: None,
    }))
}

pub fn make_if_else(
    predicate: ast::Expr,
    consequent: ast::Expr,
    alternative: ast::Expr,
) -> ast::Expr {
    ast::Expr::IfStatement(make_node(ast::IfStatement {
        predicate: make_node(ast::Predicate::One(Box::new(make_node(predicate)))),
        consequent: make_node(ast::Block(vec![make_node(consequent)])),
        alternative: Some(make_node(ast::Block(vec![make_node(alternative)]))),
    }))
}

pub fn make_coalesce(op1: ast::Expr, op2: ast::Expr) -> ast::Expr {
    make_op(make_node(op1), Opcode::Err, make_node(op2))
}

pub fn make_block(exprs: Vec<ast::Expr>) -> ast::Expr {
    ast::Expr::Container(make_node(ast::Container::Block(make_node(ast::Block(
        exprs
            .iter()
            .map(|expr| make_node(expr.clone()))
            .collect::<Vec<Node<Expr>>>(),
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
pub fn make_function_call<T: IntoIterator<Item = ast::Expr>>(
    tag: &str,
    arguments: T,
    abort_on_error: bool,
) -> ast::Expr {
    ast::Expr::FunctionCall(make_node(ast::FunctionCall {
        ident: make_node(ast::Ident::new(tag.to_string())),
        abort_on_error,
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
