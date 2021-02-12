use crate::expression::*;
use crate::{Function, Program, State};
use chrono::{TimeZone, Utc};
use diagnostic::DiagnosticError;
use ordered_float::NotNan;
use parser::ast::{self, Node};
use std::convert::TryFrom;

pub type Errors = Vec<Box<dyn DiagnosticError>>;

pub struct Compiler<'a> {
    fns: &'a [Box<dyn Function>],
    state: &'a mut State,
    errors: Errors,
}

impl<'a> Compiler<'a> {
    pub(super) fn new(fns: &'a [Box<dyn Function>], state: &'a mut State) -> Self {
        Self {
            fns,
            state,
            errors: vec![],
        }
    }

    pub(super) fn compile(mut self, ast: parser::Program) -> Result<Program, Errors> {
        let expressions = self
            .compile_root_exprs(ast)
            .into_iter()
            .map(|expr| Box::new(expr) as _)
            .collect();

        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        Ok(Program(expressions))
    }

    fn compile_root_exprs(
        &mut self,
        nodes: impl IntoIterator<Item = Node<ast::RootExpr>>,
    ) -> Vec<Expr> {
        use ast::RootExpr::*;

        nodes
            .into_iter()
            .filter_map(|node| {
                let span = node.span();

                match node.into_inner() {
                    Expr(expr) => {
                        let expr = self.compile_expr(expr);
                        if expr.type_def(self.state).is_fallible() {
                            use crate::expression::Error;
                            let err = Error::Fallible { span };
                            self.errors.push(Box::new(err));
                        }

                        Some(expr)
                    }
                    Error(err) => {
                        self.handle_parser_error(err);
                        None
                    }
                }
            })
            .collect()
    }

    fn compile_exprs(&mut self, nodes: impl IntoIterator<Item = Node<ast::Expr>>) -> Vec<Expr> {
        nodes
            .into_iter()
            .map(|node| self.compile_expr(node))
            .collect()
    }

    fn compile_expr(&mut self, node: Node<ast::Expr>) -> Expr {
        use ast::Expr::*;

        match node.into_inner() {
            Literal(node) => self.compile_literal(node).into(),
            Container(node) => self.compile_container(node).into(),
            IfStatement(node) => self.compile_if_statement(node).into(),
            Op(node) => self.compile_op(node).into(),
            Assignment(node) => self.compile_assignment(node).into(),
            Query(node) => self.compile_query(node).into(),
            FunctionCall(node) => self.compile_function_call(node).into(),
            Variable(node) => self.compile_variable(node).into(),
            Unary(node) => self.compile_unary(node).into(),
        }
    }

    fn compile_literal(&mut self, node: Node<ast::Literal>) -> Literal {
        use literal::ErrorVariant::*;

        Literal::try_from(node).unwrap_or_else(|err| {
            let value = match &err.variant {
                #[allow(clippy::trivial_regex)]
                InvalidRegex(_) => regex::Regex::new("").unwrap().into(),
                InvalidTimestamp(..) => Utc.timestamp(0, 0).into(),
                NanFloat => NotNan::new(0.0).unwrap().into(),
            };

            self.errors.push(Box::new(err));
            value
        })
    }

    fn compile_container(&mut self, node: Node<ast::Container>) -> Container {
        use ast::Container::*;

        let variant = match node.into_inner() {
            Group(node) => self.compile_group(*node).into(),
            Block(node) => self.compile_block(node).into(),
            Array(node) => self.compile_array(node).into(),
            Object(node) => self.compile_object(node).into(),
        };

        Container::new(variant)
    }

    fn compile_group(&mut self, node: Node<ast::Group>) -> Group {
        let expr = self.compile_expr(node.into_inner().into_inner());

        Group::new(expr)
    }

    fn compile_block(&mut self, node: Node<ast::Block>) -> Block {
        let exprs = self.compile_exprs(node.into_inner().into_iter());

        Block::new(exprs)
    }

    fn compile_array(&mut self, node: Node<ast::Array>) -> Array {
        let exprs = self.compile_exprs(node.into_inner().into_iter());

        Array::new(exprs)
    }

    fn compile_object(&mut self, node: Node<ast::Object>) -> Object {
        use std::collections::BTreeMap;

        let exprs = node
            .into_inner()
            .into_iter()
            .map(|(k, expr)| (k.into_inner(), self.compile_expr(expr)))
            .collect::<BTreeMap<_, _>>();

        Object::new(exprs)
    }

    fn compile_if_statement(&mut self, node: Node<ast::IfStatement>) -> IfStatement {
        let ast::IfStatement {
            predicate,
            consequent,
            alternative,
        } = node.into_inner();

        let predicate = match self.compile_predicate(predicate) {
            Ok(v) => v,
            Err(err) => {
                self.errors.push(Box::new(err));
                return IfStatement::noop();
            }
        };

        let consequent = self.compile_block(consequent);
        let alternative = alternative.map(|block| self.compile_block(block));

        IfStatement {
            predicate,
            consequent,
            alternative,
        }
    }

    fn compile_predicate(&mut self, node: Node<ast::Predicate>) -> predicate::Result {
        use ast::Predicate::*;

        let (span, predicate) = node.take();

        let exprs = match predicate {
            One(node) => vec![self.compile_expr(*node)],
            Many(nodes) => self.compile_exprs(nodes),
        };

        Predicate::new(Node::new(span, Block::new(exprs)), &self.state)
    }

    fn compile_op(&mut self, node: Node<ast::Op>) -> Op {
        let op = node.into_inner();
        let ast::Op(lhs, opcode, rhs) = op;

        let lhs = self.compile_expr(*lhs);
        let rhs = self.compile_expr(*rhs);

        Op::new(lhs, opcode, rhs).unwrap_or_else(|err| {
            self.errors.push(Box::new(err));
            Op::noop()
        })
    }

    fn compile_assignment(&mut self, node: Node<ast::Assignment>) -> Assignment {
        use assignment::Variant;
        use ast::Assignment::*;

        self.state.snapshot();

        let node = node.map(|assignment| match assignment {
            Single { target, expr } => {
                let span = expr.span();
                let expr = Box::new(expr.map(|node| self.compile_expr(Node::new(span, node))));

                Variant::Single { target, expr }
            }
            Infallible { ok, err, expr } => {
                let span = expr.span();
                let expr = Box::new(expr.map(|node| self.compile_expr(Node::new(span, node))));

                Variant::Infallible { ok, err, expr }
            }
        });

        Assignment::new(node, &mut self.state).unwrap_or_else(|err| {
            self.state.rollback();
            self.errors.push(Box::new(err));
            Assignment::noop()
        })
    }

    fn compile_query(&mut self, node: Node<ast::Query>) -> Query {
        let ast::Query { target, path } = node.into_inner();
        let target = self.compile_query_target(target);

        Query::new(target, path.into_inner().into())
    }

    fn compile_query_target(&mut self, node: Node<ast::QueryTarget>) -> query::Target {
        use ast::QueryTarget::*;
        use query::Target;

        let span = node.span();

        match node.into_inner() {
            External => Target::External,
            Internal(ident) => {
                let variable = self.compile_variable(Node::new(span, ident));
                Target::Internal(variable)
            }
            Container(container) => {
                let container = self.compile_container(Node::new(span, container));
                Target::Container(container)
            }
            FunctionCall(call) => {
                let call = self.compile_function_call(Node::new(span, call));
                Target::FunctionCall(call)
            }
        }
    }

    fn compile_function_call(&mut self, node: Node<ast::FunctionCall>) -> FunctionCall {
        let call_span = node.span();
        let ast::FunctionCall {
            ident,
            abort_on_error,
            arguments,
        } = node.into_inner();

        let arguments = arguments
            .into_iter()
            .map(|node| Node::new(node.span(), self.compile_function_argument(node)))
            .collect();

        FunctionCall::new(
            call_span,
            ident,
            abort_on_error,
            arguments,
            self.fns,
            self.state,
        )
        .unwrap_or_else(|err| {
            self.errors.push(Box::new(err));
            FunctionCall::noop()
        })
    }

    fn compile_function_argument(&mut self, node: Node<ast::FunctionArgument>) -> FunctionArgument {
        let ast::FunctionArgument { ident, expr } = node.into_inner();
        let expr = Node::new(expr.span(), self.compile_expr(expr));
        FunctionArgument::new(ident, expr)
    }

    fn compile_variable(&mut self, node: Node<ast::Ident>) -> Variable {
        Variable::new(node.into_inner(), &self.state)
    }

    fn compile_unary(&mut self, node: Node<ast::Unary>) -> Unary {
        use ast::Unary::*;

        let variant = match node.into_inner() {
            Not(node) => self.compile_not(node).into(),
        };

        Unary::new(variant)
    }

    fn compile_not(&mut self, node: Node<ast::Not>) -> Not {
        let (not, expr) = node.into_inner().take();

        let node = Node::new(expr.span(), self.compile_expr(*expr));

        Not::new(node, not.span(), &self.state).unwrap_or_else(|err| {
            self.errors.push(Box::new(err));
            Not::noop()
        })
    }

    fn handle_parser_error(&mut self, error: parser::Error) {
        self.errors.push(Box::new(error))
    }
}
