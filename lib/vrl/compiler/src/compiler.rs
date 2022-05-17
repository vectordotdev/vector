use diagnostic::{DiagnosticList, DiagnosticMessage, Severity, Span};
use lookup::LookupBuf;
use parser::ast::{self, Node};

use crate::{
    expression::*,
    program::ProgramInfo,
    state::{ExternalEnv, LocalEnv},
    Function, Program,
};

pub(crate) type Diagnostics = Vec<Box<dyn DiagnosticMessage>>;

pub(crate) struct Compiler<'a> {
    fns: &'a [Box<dyn Function>],
    diagnostics: Diagnostics,
    fallible: bool,
    abortable: bool,
    local: LocalEnv,
    external_queries: Vec<LookupBuf>,
    external_assignments: Vec<LookupBuf>,
}

impl<'a> Compiler<'a> {
    pub(super) fn new(fns: &'a [Box<dyn Function>]) -> Self {
        Self {
            fns,
            diagnostics: vec![],
            fallible: false,
            abortable: false,
            local: LocalEnv::default(),
            external_queries: vec![],
            external_assignments: vec![],
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
    ) -> Result<(Program, DiagnosticList), DiagnosticList> {
        let mut expressions = self.compile_root_exprs(ast, external);

        if expressions.is_empty() {
            expressions.push(Expr::Noop(Noop));
        }

        let (errors, warnings): (Vec<_>, Vec<_>) =
            self.diagnostics.into_iter().partition(|diagnostic| {
                matches!(diagnostic.severity(), Severity::Bug | Severity::Error)
            });

        if !errors.is_empty() {
            return Err(errors.into());
        }

        let info = ProgramInfo {
            fallible: self.fallible,
            abortable: self.abortable,
            target_queries: self.external_queries,
            target_assignments: self.external_assignments,
        };

        let expressions = Block::new(expressions, self.local);

        Ok((Program { expressions, info }, warnings.into()))
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
                            self.diagnostics.push(Box::new(err));
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
            Literal(node) => self.compile_literal(node, external),
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

    #[cfg(feature = "expr-literal")]
    fn compile_literal(&mut self, node: Node<ast::Literal>, external: &mut ExternalEnv) -> Expr {
        use ast::Literal::*;
        use bytes::Bytes;
        use chrono::{TimeZone, Utc};
        use literal::ErrorVariant::*;
        use ordered_float::NotNan;

        let (span, lit) = node.take();

        let literal = match lit {
            String(template) => {
                if let Some(v) = template.as_literal_string() {
                    Ok(Literal::String(Bytes::from(v.to_string())))
                } else {
                    // Rewrite the template into an expression and compile that block.
                    return self.compile_expr(
                        Node::new(span, template.rewrite_to_concatenated_strings()),
                        external,
                    );
                }
            }
            RawString(v) => Ok(Literal::String(Bytes::from(v))),
            Integer(v) => Ok(Literal::Integer(v)),
            Float(v) => Ok(Literal::Float(v)),
            Boolean(v) => Ok(Literal::Boolean(v)),
            Regex(v) => regex::Regex::new(&v)
                .map_err(|err| literal::Error::from((span, err)))
                .map(|r| Literal::Regex(r.into())),
            // TODO: support more formats (similar to Vector's `Convert` logic)
            Timestamp(v) => v
                .parse()
                .map(Literal::Timestamp)
                .map_err(|err| literal::Error::from((span, err))),
            Null => Ok(Literal::Null),
        };

        let literal = literal.unwrap_or_else(|err| {
            let value = match &err.variant {
                #[allow(clippy::trivial_regex)]
                InvalidRegex(_) => regex::Regex::new("").unwrap().into(),
                InvalidTimestamp(..) => Utc.timestamp(0, 0).into(),
                NanFloat => NotNan::new(0.0).unwrap().into(),
            };

            self.diagnostics.push(Box::new(err));
            value
        });

        literal.into()
    }

    #[cfg(not(feature = "expr-literal"))]
    fn compile_literal(&mut self, node: Node<ast::Literal>, _: &mut ExternalEnv) -> Expr {
        self.handle_missing_feature_error(node.span(), "expr-literal")
            .into()
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
        // We get a copy of the current local state, so that we can use it to
        // remove any *new* state added in the block, as that state is lexically
        // scoped to the block, and must not be visible to the rest of the
        // program.
        let local_snapshot = self.local.clone();

        // We can now start compiling the expressions within the block, which
        // will use the existing local state of the compiler, as blocks have
        // access to any state of their parent expressions.
        let exprs = self.compile_exprs(node.into_inner().into_iter(), external);

        // Now that we've compiled the expressions, we pass them into the block,
        // and also a copy of the local state, which includes any state added by
        // the compiled expressions in the block.
        let block = Block::new(exprs, self.local.clone());

        // Take the local state snapshot captured before we started compiling
        // the block, and merge back into it any mutations that happened to
        // state the snapshot was already tracking. Then, revert the compiler
        // local state to the updated snapshot.
        self.local = local_snapshot.merge_mutations(self.local.clone());

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

    #[cfg(feature = "expr-if_statement")]
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
                self.diagnostics.push(Box::new(err));
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

    #[cfg(not(feature = "expr-if_statement"))]
    fn compile_if_statement(&mut self, node: Node<ast::IfStatement>, _: &mut ExternalEnv) -> Noop {
        self.handle_missing_feature_error(node.span(), "expr-if_statement")
    }

    #[cfg(feature = "expr-if_statement")]
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

        Predicate::new(
            Node::new(span, exprs),
            (&self.local, external),
            &mut self.diagnostics,
        )
    }

    #[cfg(feature = "expr-op")]
    fn compile_op(&mut self, node: Node<ast::Op>, external: &mut ExternalEnv) -> Op {
        let op = node.into_inner();
        let ast::Op(lhs, opcode, rhs) = op;

        let lhs_span = lhs.span();
        let lhs = Node::new(lhs_span, self.compile_expr(*lhs, external));

        let rhs_span = rhs.span();
        let rhs = Node::new(rhs_span, self.compile_expr(*rhs, external));

        Op::new(lhs, opcode, rhs, (&mut self.local, external)).unwrap_or_else(|err| {
            self.diagnostics.push(Box::new(err));
            Op::noop()
        })
    }

    #[cfg(not(feature = "expr-op"))]
    fn compile_op(&mut self, node: Node<ast::Op>, _: &mut ExternalEnv) -> Noop {
        self.handle_missing_feature_error(node.span(), "expr-op")
    }

    /// Rewrites the ast for `a |= b` to be `a = a | b`.
    #[cfg(feature = "expr-assignment")]
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

    #[cfg(feature = "expr-assignment")]
    fn compile_assignment(
        &mut self,
        node: Node<ast::Assignment>,
        external: &mut ExternalEnv,
    ) -> Assignment {
        use assignment::Variant;
        use ast::{Assignment::*, AssignmentOp};
        use value::Value;

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

        let assignment = Assignment::new(node, &mut self.local, external).unwrap_or_else(|err| {
            self.diagnostics.push(Box::new(err));
            Assignment::noop()
        });

        // Track any potential external target assignments within the program.
        //
        // This data is exposed to the caller of the compiler, to allow any
        // potential external optimizations.
        for target in assignment.targets() {
            if let assignment::Target::External(path) = target {
                self.external_assignments.push(path);
            }
        }

        assignment
    }

    #[cfg(not(feature = "expr-assignment"))]
    fn compile_assignment(&mut self, node: Node<ast::Assignment>, _: &mut ExternalEnv) -> Noop {
        self.handle_missing_feature_error(node.span(), "expr-assignment")
    }

    #[cfg(feature = "expr-query")]
    fn compile_query(&mut self, node: Node<ast::Query>, external: &mut ExternalEnv) -> Query {
        let ast::Query { target, path } = node.into_inner();
        let path = path.into_inner();
        let target = self.compile_query_target(target, external);

        // Track any potential external target queries within the program.
        //
        // This data is exposed to the caller of the compiler, to allow any
        // potential external optimizations.
        if let Target::External = target {
            self.external_queries.push(path.clone())
        }

        Query::new(target, path)
    }

    #[cfg(not(feature = "expr-query"))]
    fn compile_query(&mut self, node: Node<ast::Query>, _: &mut ExternalEnv) -> Noop {
        self.handle_missing_feature_error(node.span(), "expr-query")
    }

    #[cfg(feature = "expr-query")]
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

    #[cfg(feature = "expr-function_call")]
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
            closure,
        } = node.into_inner();

        // TODO: Remove this (hacky) code once dynamic path syntax lands.
        //
        // See: https://github.com/vectordotdev/vector/issues/12547
        if ident.as_deref() == "get" {
            self.external_queries.push(LookupBuf::root())
        }

        let arguments = arguments
            .into_iter()
            .map(|node| Node::new(node.span(), self.compile_function_argument(node, external)))
            .collect();

        if abort_on_error {
            self.fallible = true;
        }

        let (closure_variables, closure_block) = match closure {
            Some(closure) => {
                let span = closure.span();
                let ast::FunctionClosure { variables, block } = closure.into_inner();
                (Some(Node::new(span, variables)), Some(block))
            }
            None => (None, None),
        };

        // Keep track of the known scope *before* we compile the closure.
        //
        // This allows us to revert to any known state that the closure
        // arguments might overwrite.
        let local_snapshot = self.local.clone();

        // First, we create a new function-call builder to validate the
        // expression.
        function_call::Builder::new(
            call_span,
            ident,
            abort_on_error,
            arguments,
            self.fns,
            &mut self.local,
            external,
            closure_variables,
        )
        // Then, we compile the closure block, and compile the final
        // function-call expression, including the attached closure.
        .and_then(|builder| {
            let block = closure_block.map(|block| {
                let span = block.span();
                let block = self.compile_block(block, external);

                Node::new(span, block)
            });

            builder.compile(&mut self.local, external, block, local_snapshot)
        })
        .unwrap_or_else(|err| {
            self.diagnostics.push(Box::new(err));
            FunctionCall::noop()
        })
    }

    #[cfg(feature = "expr-function_call")]
    fn compile_function_argument(
        &mut self,
        node: Node<ast::FunctionArgument>,
        external: &mut ExternalEnv,
    ) -> FunctionArgument {
        let ast::FunctionArgument { ident, expr } = node.into_inner();
        let expr = Node::new(expr.span(), self.compile_expr(expr, external));
        FunctionArgument::new(ident, expr)
    }

    #[cfg(not(feature = "expr-function_call"))]
    fn compile_function_call(
        &mut self,
        node: Node<ast::FunctionCall>,
        _: &mut ExternalEnv,
    ) -> Noop {
        // Guard against `dead_code` lint, to avoid having to sprinkle
        // attributes all over the place.
        let _ = self.fns;

        self.handle_missing_feature_error(node.span(), "expr-function_call")
    }

    fn compile_variable(
        &mut self,
        node: Node<ast::Ident>,
        _external: &mut ExternalEnv,
    ) -> Variable {
        let (span, ident) = node.take();

        Variable::new(span, ident.clone(), &self.local).unwrap_or_else(|err| {
            self.diagnostics.push(Box::new(err));
            Variable::noop(ident)
        })
    }

    #[cfg(feature = "expr-unary")]
    fn compile_unary(&mut self, node: Node<ast::Unary>, external: &mut ExternalEnv) -> Unary {
        use ast::Unary::*;

        let variant = match node.into_inner() {
            Not(node) => self.compile_not(node, external).into(),
        };

        Unary::new(variant)
    }

    #[cfg(not(feature = "expr-unary"))]
    fn compile_unary(&mut self, node: Node<ast::Unary>, _: &mut ExternalEnv) -> Noop {
        use ast::Unary::*;

        let span = match node.into_inner() {
            Not(node) => node.take().1.take().0,
        };

        self.handle_missing_feature_error(span.span(), "expr-unary")
    }

    #[cfg(feature = "expr-unary")]
    fn compile_not(&mut self, node: Node<ast::Not>, external: &mut ExternalEnv) -> Not {
        let (not, expr) = node.into_inner().take();

        let node = Node::new(expr.span(), self.compile_expr(*expr, external));

        Not::new(node, not.span(), (&self.local, external)).unwrap_or_else(|err| {
            self.diagnostics.push(Box::new(err));
            Not::noop()
        })
    }

    #[cfg(feature = "expr-abort")]
    fn compile_abort(&mut self, node: Node<ast::Abort>, external: &mut ExternalEnv) -> Abort {
        self.abortable = true;
        let (span, abort) = node.take();
        let message = abort
            .message
            .map(|expr| Node::new(expr.span(), self.compile_expr(*expr, external)));

        Abort::new(span, message, (&self.local, external)).unwrap_or_else(|err| {
            self.diagnostics.push(Box::new(err));
            Abort::noop(span)
        })
    }

    #[cfg(not(feature = "expr-abort"))]
    fn compile_abort(&mut self, node: Node<ast::Abort>, _: &mut ExternalEnv) -> Noop {
        self.handle_missing_feature_error(node.span(), "expr-abort")
    }

    fn handle_parser_error(&mut self, error: parser::Error) {
        self.diagnostics.push(Box::new(error))
    }

    #[allow(dead_code)]
    fn handle_missing_feature_error(&mut self, span: Span, feature: &'static str) -> Noop {
        self.diagnostics
            .push(Box::new(Error::Missing { span, feature }));

        Noop
    }
}
