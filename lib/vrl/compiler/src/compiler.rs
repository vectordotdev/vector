use std::convert::TryFrom;

use chrono::{TimeZone, Utc};
use diagnostic::DiagnosticError;
use ordered_float::NotNan;
use parser::ast::{self, AssignmentOp, Node};

use crate::{
    expression::*,
    state::{ExternalEnv, LocalEnv},
    Function, Program, Value,
};

pub(crate) type Errors = Vec<Box<dyn DiagnosticError>>;

pub(crate) struct Compiler<'a> {
    fns: &'a [Box<dyn Function>],
    errors: Errors,
    fallible: bool,
    abortable: bool,
    local: LocalEnv,
}

impl<'a> Compiler<'a> {
    pub(super) fn new(fns: &'a [Box<dyn Function>]) -> Self {
        Self {
            fns,
            errors: vec![],
            fallible: false,
            abortable: false,
            local: LocalEnv::default(),
        }
    }

    /// An intenal function used by `compile_for_repl`.
    ///
    /// This should only be used for its intended purpose.
    pub(super) fn new_with_local_state(fns: &'a [Box<dyn Function>], local: LocalEnv) -> Self {
        let mut compiler = Self::new(fns);
        compiler.local = local;
        compiler
    }

    pub(super) fn compile(
        mut self,
        ast: parser::Program,
        external: &mut ExternalEnv,
    ) -> Result<(Program, LocalEnv), Errors> {
        let expressions = self
            .compile_root_exprs(ast, external)
            .into_iter()
            .map(|expr| Box::new(expr) as _)
            .collect();

        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        let program = Program {
            expressions,
            fallible: self.fallible,
            abortable: self.abortable,
        };

        Ok((program, self.local))
    }

    fn compile_root_exprs(
        &mut self,
        nodes: impl IntoIterator<Item = Node<ast::RootExpr>>,
        external: &mut ExternalEnv,
    ) -> Vec<Expr> {
        use ast::RootExpr::*;

        nodes
            .into_iter()
            .filter_map(|node| {
                let span = node.span();

                match node.into_inner() {
                    Expr(expr) => {
                        let expr = self.compile_expr(expr, external);
                        if expr.type_def((&self.local, external)).is_fallible() {
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

    fn compile_exprs(
        &mut self,
        nodes: impl IntoIterator<Item = Node<ast::Expr>>,
        external: &mut ExternalEnv,
    ) -> Vec<Expr> {
        nodes
            .into_iter()
            .map(|node| self.compile_expr(node, external))
            .collect()
    }

    fn compile_expr(&mut self, node: Node<ast::Expr>, external: &mut ExternalEnv) -> Expr {
        use ast::Expr::*;

        match node.into_inner() {
            Literal(node) => self.compile_literal(node).into(),
            Container(node) => self.compile_container(node, external).into(),
            IfStatement(node) => self.compile_if_statement(node, external).into(),
            Op(node) => self.compile_op(node, external).into(),
            Assignment(node) => self.compile_assignment(node, external).into(),
            Query(node) => self.compile_query(node, external).into(),
            FunctionCall(node) => self.compile_function_call(node, external).into(),
            Variable(node) => self.compile_variable(node, external).into(),
            Unary(node) => self.compile_unary(node, external).into(),
            Abort(node) => self.compile_abort(node, external).into(),
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

    fn compile_container(
        &mut self,
        node: Node<ast::Container>,
        external: &mut ExternalEnv,
    ) -> Container {
        use ast::Container::*;

        let variant = match node.into_inner() {
            Group(node) => self.compile_group(*node, external).into(),
            Block(node) => self.compile_block(node, external).into(),
            Array(node) => self.compile_array(node, external).into(),
            Object(node) => self.compile_object(node, external).into(),
        };

        Container::new(variant)
    }

    fn compile_group(&mut self, node: Node<ast::Group>, external: &mut ExternalEnv) -> Group {
        let expr = self.compile_expr(node.into_inner().into_inner(), external);

        Group::new(expr)
    }

    fn compile_block(&mut self, node: Node<ast::Block>, external: &mut ExternalEnv) -> Block {
        // We track the original local state, as any mutations within the block
        // are removed after the block returns.
        let local = self.local.clone();

        let exprs = self.compile_exprs(node.into_inner().into_iter(), external);
        let block = Block::new(exprs, self.local.clone());

        self.local = local;
        block
    }

    fn compile_array(&mut self, node: Node<ast::Array>, external: &mut ExternalEnv) -> Array {
        let exprs = self.compile_exprs(node.into_inner().into_iter(), external);

        Array::new(exprs)
    }

    fn compile_object(&mut self, node: Node<ast::Object>, external: &mut ExternalEnv) -> Object {
        use std::collections::BTreeMap;

        let exprs = node
            .into_inner()
            .into_iter()
            .map(|(k, expr)| (k.into_inner(), self.compile_expr(expr, external)))
            .collect::<BTreeMap<_, _>>();

        Object::new(exprs)
    }

    fn compile_if_statement(
        &mut self,
        node: Node<ast::IfStatement>,
        external: &mut ExternalEnv,
    ) -> IfStatement {
        let ast::IfStatement {
            predicate,
            consequent,
            alternative,
        } = node.into_inner();

        let predicate = match self.compile_predicate(predicate, external) {
            Ok(v) => v,
            Err(err) => {
                self.errors.push(Box::new(err));
                return IfStatement::noop();
            }
        };

        let consequent = self.compile_block(consequent, external);
        let alternative = alternative.map(|block| self.compile_block(block, external));

        IfStatement {
            predicate,
            consequent,
            alternative,
        }
    }

    fn compile_predicate(
        &mut self,
        node: Node<ast::Predicate>,
        external: &mut ExternalEnv,
    ) -> predicate::Result {
        use ast::Predicate::*;

        let (span, predicate) = node.take();

        let exprs = match predicate {
            One(node) => vec![self.compile_expr(*node, external)],
            Many(nodes) => self.compile_exprs(nodes, external),
        };

        Predicate::new(Node::new(span, exprs), (&self.local, external))
    }

    fn compile_op(&mut self, node: Node<ast::Op>, external: &mut ExternalEnv) -> Op {
        let op = node.into_inner();
        let ast::Op(lhs, opcode, rhs) = op;

        let lhs_span = lhs.span();
        let lhs = Node::new(lhs_span, self.compile_expr(*lhs, external));

        let rhs_span = rhs.span();
        let rhs = Node::new(rhs_span, self.compile_expr(*rhs, external));

        Op::new(lhs, opcode, rhs, (&mut self.local, external)).unwrap_or_else(|err| {
            self.errors.push(Box::new(err));
            Op::noop()
        })
    }

    /// Rewrites the ast for `a |= b` to be `a = a | b`.
    fn rewrite_to_merge(
        &mut self,
        span: diagnostic::Span,
        target: &Node<ast::AssignmentTarget>,
        expr: Box<Node<ast::Expr>>,
        external: &mut ExternalEnv,
    ) -> Box<Node<Expr>> {
        Box::new(Node::new(
            span,
            Expr::Op(self.compile_op(
                Node::new(
                    span,
                    ast::Op(
                        Box::new(Node::new(target.span(), target.inner().to_expr(span))),
                        Node::new(span, ast::Opcode::Merge),
                        expr,
                    ),
                ),
                external,
            )),
        ))
    }

    fn compile_assignment(
        &mut self,
        node: Node<ast::Assignment>,
        external: &mut ExternalEnv,
    ) -> Assignment {
        use assignment::Variant;
        use ast::Assignment::*;

        let assignment = node.into_inner();

        let node = match assignment {
            Single { target, op, expr } => {
                let span = expr.span();

                match op {
                    AssignmentOp::Assign => {
                        let expr = Box::new(
                            expr.map(|node| self.compile_expr(Node::new(span, node), external)),
                        );

                        Node::new(span, Variant::Single { target, expr })
                    }
                    AssignmentOp::Merge => {
                        let expr = self.rewrite_to_merge(span, &target, expr, external);
                        Node::new(span, Variant::Single { target, expr })
                    }
                }
            }
            Infallible { ok, err, op, expr } => {
                let span = expr.span();

                match op {
                    AssignmentOp::Assign => {
                        let expr = Box::new(
                            expr.map(|node| self.compile_expr(Node::new(span, node), external)),
                        );
                        let node = Variant::Infallible {
                            ok,
                            err,
                            expr,
                            default: Value::Null,
                        };
                        Node::new(span, node)
                    }
                    AssignmentOp::Merge => {
                        let expr = self.rewrite_to_merge(span, &ok, expr, external);
                        let node = Variant::Infallible {
                            ok,
                            err,
                            expr,
                            default: Value::Null,
                        };

                        Node::new(span, node)
                    }
                }
            }
        };

        Assignment::new(node, &mut self.local, external).unwrap_or_else(|err| {
            self.errors.push(Box::new(err));
            Assignment::noop()
        })
    }

    fn compile_query(&mut self, node: Node<ast::Query>, external: &mut ExternalEnv) -> Query {
        let ast::Query { target, path } = node.into_inner();
        let target = self.compile_query_target(target, external);

        Query::new(target, path.into_inner())
    }

    fn compile_query_target(
        &mut self,
        node: Node<ast::QueryTarget>,
        external: &mut ExternalEnv,
    ) -> query::Target {
        use ast::QueryTarget::*;

        let span = node.span();

        match node.into_inner() {
            External => Target::External,
            Internal(ident) => {
                let variable = self.compile_variable(Node::new(span, ident), external);
                Target::Internal(variable)
            }
            Container(container) => {
                let container = self.compile_container(Node::new(span, container), external);
                Target::Container(container)
            }
            FunctionCall(call) => {
                let call = self.compile_function_call(Node::new(span, call), external);
                Target::FunctionCall(call)
            }
        }
    }

    fn compile_function_call(
        &mut self,
        node: Node<ast::FunctionCall>,
        external: &mut ExternalEnv,
    ) -> FunctionCall {
        let call_span = node.span();
        let ast::FunctionCall {
            ident,
            abort_on_error,
            arguments,
        } = node.into_inner();

        let arguments = arguments
            .into_iter()
            .map(|node| Node::new(node.span(), self.compile_function_argument(node, external)))
            .collect();

        if abort_on_error {
            self.fallible = true;
        }

        FunctionCall::new(
            call_span,
            ident,
            abort_on_error,
            arguments,
            self.fns,
            &mut self.local,
            external,
        )
        .unwrap_or_else(|err| {
            self.errors.push(Box::new(err));
            FunctionCall::noop()
        })
    }

    fn compile_function_argument(
        &mut self,
        node: Node<ast::FunctionArgument>,
        external: &mut ExternalEnv,
    ) -> FunctionArgument {
        let ast::FunctionArgument { ident, expr } = node.into_inner();
        let expr = Node::new(expr.span(), self.compile_expr(expr, external));
        FunctionArgument::new(ident, expr)
    }

    fn compile_variable(
        &mut self,
        node: Node<ast::Ident>,
        _external: &mut ExternalEnv,
    ) -> Variable {
        let (span, ident) = node.take();

        Variable::new(span, ident.clone(), &self.local).unwrap_or_else(|err| {
            self.errors.push(Box::new(err));
            Variable::noop(ident)
        })
    }

    fn compile_unary(&mut self, node: Node<ast::Unary>, external: &mut ExternalEnv) -> Unary {
        use ast::Unary::*;

        let variant = match node.into_inner() {
            Not(node) => self.compile_not(node, external).into(),
        };

        Unary::new(variant)
    }

    fn compile_not(&mut self, node: Node<ast::Not>, external: &mut ExternalEnv) -> Not {
        let (not, expr) = node.into_inner().take();

        let node = Node::new(expr.span(), self.compile_expr(*expr, external));

        Not::new(node, not.span(), (&self.local, external)).unwrap_or_else(|err| {
            self.errors.push(Box::new(err));
            Not::noop()
        })
    }

    fn compile_abort(&mut self, node: Node<ast::Abort>, external: &mut ExternalEnv) -> Abort {
        self.abortable = true;
        let (span, abort) = node.take();
        let message = abort
            .message
            .map(|expr| Node::new(expr.span(), self.compile_expr(*expr, external)));

        Abort::new(span, message, (&self.local, external)).unwrap_or_else(|err| {
            self.errors.push(Box::new(err));
            Abort::noop(span)
        })
    }

    fn handle_parser_error(&mut self, error: parser::Error) {
        self.errors.push(Box::new(error))
    }
}
