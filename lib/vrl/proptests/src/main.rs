#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(clippy::print_stderr)] // test framework
#![allow(clippy::print_stdout)] // test framework
use diagnostic::Span;
use lookup::lookup_v2::OwnedSegment;
use lookup::{FieldBuf, LookupBuf, OwnedValuePath, SegmentBuf};
use ordered_float::NotNan;
use parser::ast::{
    Assignment, AssignmentOp, AssignmentTarget, Block, Container, Expr, FunctionArgument,
    FunctionCall, Group, Ident, IfStatement, Literal, Node, Op, Opcode, Predicate, Program, Query,
    QueryTarget, RootExpr,
};
use proptest::prelude::*;

static RESERVED: &[&str] = &[
    "if",
    "for",
    "else",
    "true",
    "false",
    "null",
    "abort",
    "array",
    "bool",
    "boolean",
    "break",
    "continue",
    "do",
    "emit",
    "float",
    "for",
    "forall",
    "foreach",
    "all",
    "each",
    "any",
    "try",
    "undefined",
    "int",
    "integer",
    "iter",
    "object",
    "regex",
    "return",
    "string",
    "traverse",
    "timestamp",
    "duration",
    "unless",
    "walk",
    "while",
    "loop",
];

fn main() {
    let source = "upcase(\").\")";
    let program = parser::parse(source).unwrap();

    println!("{:?}", program);
    println!("{}", program);
}

prop_compose! {
    fn identifier()
        (ident in "[a-zA-Z_]+[a-zA-Z0-9_]+"
         .prop_filter("idents can't be reserved names or a single underscore",
                      |i| !RESERVED.iter().any(|r| *r == i) &&
                          i != "_"))
    -> String {
            ident
        }
}

prop_compose! {
    fn variable()(ident in ident(), lookup in path()) -> (Ident, OwnedValuePath) {
        (ident, lookup)
    }
}

prop_compose! {
    fn string_literal()(val in "[^\"\\\\\\)\\}]*") -> Literal {
        Literal::RawString(val.replace('\\', "\\\\").replace('\'', "\\\'"))
    }
}

prop_compose! {
    fn int_literal()(val in 0..i64::MAX) -> Literal {
        Literal::Integer(val)
    }
}

prop_compose! {
    fn timestamp_literal() (secs in 0..i64::MAX) -> Literal {
        use chrono::{Utc, TimeZone};
        Literal::Timestamp(Utc.timestamp_opt(secs, 0).single().expect("invalid timestamp").to_string())
    }
}

prop_compose! {
    fn float_literal()(numerator in 0f64..1000.0, denominator in 0f64..1000.0) -> Literal {
        Literal::Float(NotNan::new(numerator / denominator).unwrap())
    }
}

fn literal() -> impl Strategy<Value = Literal> {
    prop_oneof![string_literal(), int_literal(), float_literal()]
}

prop_compose! {
    fn ident()
        (ident in identifier())
         -> Ident {
            Ident::new(ident)
        }
}

prop_compose! {
    fn path() (path in prop::collection::vec(ident(), 1..2)) -> OwnedValuePath {
        OwnedValuePath {
            segments:
            path.into_iter()
                .map(|field| OwnedSegment::Field(field.as_ref().to_owned()))
                .collect(),
        }
    }
}

prop_compose! {
    fn query() (ident in ident(), path in path()) -> Query {
        Query {
            target: node(QueryTarget::Internal(ident)),
            path: node(path)
        }
    }
}

prop_compose! {
    fn function_call() (ident in ident(),
                        abort in prop::bool::ANY,
                        params in prop::collection::vec(ident(), 1..100)) -> FunctionCall {
        FunctionCall {
            ident: node(ident),
            abort_on_error: abort,
            arguments: params.into_iter().map(|p| node(FunctionArgument {
                ident: None,
                expr: node(Expr::Variable(node(p)))
            })).collect(),
            closure: None,
        }
    }
}

prop_compose! {
    fn single_predicate() (query in query()) -> Predicate {
        Predicate::One(Box::new(node(Expr::Query(node(
                            query,
                        )))))
    }
}

prop_compose! {
    fn many_predicate() (exprs in prop::collection::vec(expr(), 1..30)) -> Predicate {
        Predicate::Many(exprs.into_iter().map(node).collect())
    }
}

fn predicate() -> impl Strategy<Value = Predicate> {
    prop_oneof![single_predicate(), many_predicate()]
}

prop_compose! {
    fn if_statement() (predicate in predicate(),
             consequent in prop::collection::vec(expr(), 1..3),
             alternative in prop::collection::vec(expr(), 1..3)) -> Expr {
                    Expr::IfStatement(node(IfStatement {
                        predicate: node(predicate),
                        if_node: node(Block(consequent.into_iter().map(node).collect())),
                        else_node: Some(node(Block(alternative.into_iter().map(node).collect()))),
                    }))
    }
}

prop_compose! {
    // Just a quick hack so I don't have to make AssignmentTarget Clone.
    fn noop() (_hack in prop::bool::ANY) -> AssignmentTarget {
        AssignmentTarget::Noop
    }
}

fn assignment_target() -> impl Strategy<Value = AssignmentTarget> {
    prop_oneof![
        noop(),
        variable().prop_map(|(v, lookup)| AssignmentTarget::Internal(v, Some(lookup))),
        // TODO Paths
    ]
}

fn assignment_op() -> impl Strategy<Value = AssignmentOp> {
    prop_oneof![Just(AssignmentOp::Assign), Just(AssignmentOp::Merge),]
}

fn opcode() -> impl Strategy<Value = Opcode> {
    prop_oneof![
        Just(Opcode::Mul),
        Just(Opcode::Add),
        Just(Opcode::Sub),
        Just(Opcode::Or),
        Just(Opcode::Div),
        Just(Opcode::And),
        Just(Opcode::Err),
        Just(Opcode::Ne),
        Just(Opcode::Eq),
        Just(Opcode::Ge),
        Just(Opcode::Gt),
        Just(Opcode::Le),
        Just(Opcode::Lt),
    ]
}

/// Makes a node with a zero span
fn node<E>(expr: E) -> Node<E> {
    Node::new(Span::new(0, 0), expr)
}

/// Wraps the expression in a group container
fn container(expr: Expr) -> Expr {
    Expr::Container(node(Container::Group(Box::new(node(Group(node(expr)))))))
}

fn expr() -> impl Strategy<Value = Expr> {
    let leaf = prop_oneof![
        query().prop_map(|v| Expr::Query(node(v))),
        ident().prop_map(|v| Expr::Variable(node(v))),
        literal().prop_map(|v| Expr::Literal(node(v))),
    ];

    leaf.prop_recursive(2, 24, 3, |inner| {
        prop_oneof![
            (inner.clone(), opcode(), inner.clone()).prop_map(|(l, o, r)| Expr::Op(node(Op(
                Box::new(node(l)),
                node(o),
                Box::new(node(r))
            )))),
            (assignment_target(), assignment_op(), inner.clone()).prop_map(|(target, op, expr)| {
                container(Expr::Assignment(node(Assignment::Single {
                    target: node(target),
                    op,
                    expr: Box::new(node(expr)),
                })))
            }),
            (ident(), prop::bool::ANY, prop::collection::vec(inner, 1..3)).prop_map(
                |(ident, abort_on_error, arguments)| {
                    Expr::FunctionCall(node(FunctionCall {
                        ident: node(ident),
                        abort_on_error,
                        arguments: arguments
                            .into_iter()
                            .map(|p| {
                                node(FunctionArgument {
                                    ident: None,
                                    expr: node(p),
                                })
                            })
                            .collect(),
                        closure: None,
                    }))
                }
            ),
        ]
    })
}

fn program(expr: Expr) -> Program {
    Program(vec![node(RootExpr::Expr(node(expr)))])
}

proptest! {
    #[test]
    fn expr_parses(expr in expr()) {
        let expr = program(expr);
        let source = expr.to_string();
        let program = parser::parse(source.clone()).unwrap();

        assert_eq!(program.to_string(),
                   expr.to_string(),
                   "{}", source);
    }

    #[test]
    fn if_parses(expr in if_statement()) {
        let expr = program(expr);
        let source = expr.to_string();
        let program = parser::parse(source.clone()).unwrap();

        assert_eq!(program.to_string(),
                   expr.to_string(),
                   "{}", source);
    }
}
