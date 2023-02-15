use core::Value;
use diagnostic::{DiagnosticList, DiagnosticMessage, Severity, Span};
use lookup::{OwnedTargetPath, OwnedValuePath, PathPrefix};
use parser::ast::{self, Node, QueryTarget};

use crate::state::TypeState;
use crate::{
    expression::{
        assignment, function_call, literal, predicate, query, Abort, Array, Assignment, Block,
        Container, Error, Expr, Expression, FunctionArgument, FunctionCall, Group, IfStatement,
        Literal, Noop, Not, Object, Op, Predicate, Query, Target, Unary, Variable,
    },
    parser::ast::RootExpr,
    program::ProgramInfo,
    CompileConfig, Function, Program, TypeDef,
};

pub(crate) type Diagnostics = Vec<Box<dyn DiagnosticMessage>>;

pub struct CompilationResult {
    pub program: Program,
    pub warnings: DiagnosticList,
    pub config: CompileConfig,
}

/// The compiler has many `compile_*` functions. These all accept a `state` param which
/// should contain the type state of the program immediately before the expression
/// that is being compiled would execute. The state should be modified to reflect the
/// state after the compiled expression executes. This logic lives in `Expression::type_info`.
pub struct Compiler<'a> {
    fns: &'a [Box<dyn Function>],
    diagnostics: Diagnostics,
    fallible: bool,
    abortable: bool,
    external_queries: Vec<OwnedTargetPath>,
    external_assignments: Vec<OwnedTargetPath>,

    /// A list of variables that are missing, because the rhs expression of the
    /// assignment failed to compile.
    ///
    /// This list allows us to avoid printing "undefined variable" compilation
    /// errors when the reason for it being undefined is another compiler error.
    skip_missing_query_target: Vec<(QueryTarget, OwnedValuePath)>,

    /// Track which expression in a chain of expressions is fallible.
    ///
    /// It is possible for this state to switch from `None`, to `Some(T)` and
    /// back to `None`, if the parent expression of a fallible expression
    /// nullifies the fallibility of that expression.
    // This should probably be kept on the call stack as the "compile_*" functions are called
    // otherwise some expressions may remove it when they shouldn't (such as the RHS of an operation removing
    // the error from the LHS)
    fallible_expression_error: Option<Box<dyn DiagnosticMessage>>,

    config: CompileConfig,
}

impl<'a> Compiler<'a> {
    pub fn compile(
        fns: &'a [Box<dyn Function>],
        ast: parser::Program,
        state: &TypeState,
        config: CompileConfig,
    ) -> Result<CompilationResult, DiagnosticList> {
        let initial_state = state.clone();
        let mut state = state.clone();

        let mut compiler = Self {
            fns,
            diagnostics: vec![],
            fallible: false,
            abortable: false,
            external_queries: vec![],
            external_assignments: vec![],
            skip_missing_query_target: vec![],
            fallible_expression_error: None,
            config,
        };
        let expressions = compiler.compile_root_exprs(ast, &mut state);

        let (errors, warnings): (Vec<_>, Vec<_>) =
            compiler.diagnostics.into_iter().partition(|diagnostic| {
                matches!(diagnostic.severity(), Severity::Bug | Severity::Error)
            });

        if !errors.is_empty() {
            return Err(errors.into());
        }

        let result = CompilationResult {
            program: Program {
                expressions: Block::new_inline(expressions),
                info: ProgramInfo {
                    fallible: compiler.fallible,
                    abortable: compiler.abortable,
                    target_queries: compiler.external_queries,
                    target_assignments: compiler.external_assignments,
                },
                initial_state,
            },
            warnings: warnings.into(),
            config: compiler.config,
        };
        Ok(result)
    }

    fn compile_exprs(
        &mut self,
        nodes: impl IntoIterator<Item = Node<ast::Expr>>,
        state: &mut TypeState,
    ) -> Option<Vec<Expr>> {
        let mut exprs = vec![];
        for node in nodes {
            let expr = self.compile_expr(node, state)?;
            exprs.push(expr);
        }
        Some(exprs)
    }

    fn compile_expr(&mut self, node: Node<ast::Expr>, state: &mut TypeState) -> Option<Expr> {
        use ast::Expr::{
            Abort, Assignment, Container, FunctionCall, IfStatement, Literal, Op, Query, Unary,
            Variable,
        };
        let original_state = state.clone();

        let span = node.span();

        let expr = match node.into_inner() {
            Literal(node) => self.compile_literal(node, state),
            Container(node) => self.compile_container(node, state).map(Into::into),
            IfStatement(node) => self.compile_if_statement(node, state).map(Into::into),
            Op(node) => self.compile_op(node, state).map(Into::into),
            Assignment(node) => self.compile_assignment(node, state).map(Into::into),
            Query(node) => self.compile_query(node, state).map(Into::into),
            FunctionCall(node) => self.compile_function_call(node, state).map(Into::into),
            Variable(node) => self.compile_variable(node, state).map(Into::into),
            Unary(node) => self.compile_unary(node, state).map(Into::into),
            Abort(node) => self.compile_abort(node, state).map(Into::into),
        }?;

        // If the previously compiled expression is fallible, _and_ we are
        // currently not tracking any existing fallible expression in the chain
        // of expressions, then this is the first expression within that chain
        // that can cause the entire chain to be fallible.

        let type_def = expr.type_info(&original_state).result;
        if type_def.is_fallible() && self.fallible_expression_error.is_none() {
            let error = crate::expression::Error::Fallible { span };
            self.fallible_expression_error = Some(Box::new(error) as _);
        }

        Some(expr)
    }

    #[cfg(feature = "expr-literal")]
    fn compile_literal(&mut self, node: Node<ast::Literal>, state: &mut TypeState) -> Option<Expr> {
        use ast::Literal::{Boolean, Float, Integer, Null, RawString, Regex, String, Timestamp};
        use bytes::Bytes;

        let (span, lit) = node.take();

        let literal = match lit {
            String(template) => {
                if let Some(v) = template.as_literal_string() {
                    Ok(Literal::String(Bytes::from(v.to_string())))
                } else {
                    // Rewrite the template into an expression and compile that block.
                    return self.compile_expr(
                        Node::new(span, template.rewrite_to_concatenated_strings()),
                        state,
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

        literal
            .map(Into::into)
            .map_err(|err| self.diagnostics.push(Box::new(err)))
            .ok()
    }

    #[cfg(not(feature = "expr-literal"))]
    fn compile_literal(&mut self, node: Node<ast::Literal>, _: &mut ExternalEnv) -> Option<Expr> {
        self.handle_missing_feature_error(node.span(), "expr-literal")
    }

    fn compile_container(
        &mut self,
        node: Node<ast::Container>,
        state: &mut TypeState,
    ) -> Option<Container> {
        use ast::Container::{Array, Block, Group, Object};

        let variant = match node.into_inner() {
            Group(node) => self.compile_group(*node, state)?.into(),
            Block(node) => self.compile_block(node, state)?.into(),
            Array(node) => self.compile_array(node, state)?.into(),
            Object(node) => self.compile_object(node, state)?.into(),
        };

        Some(Container::new(variant))
    }

    fn compile_group(&mut self, node: Node<ast::Group>, state: &mut TypeState) -> Option<Group> {
        let expr = self.compile_expr(node.into_inner().into_inner(), state)?;

        Some(Group::new(expr))
    }

    fn compile_root_exprs(
        &mut self,
        nodes: impl IntoIterator<Item = Node<ast::RootExpr>>,
        state: &mut TypeState,
    ) -> Vec<Expr> {
        let mut node_exprs = vec![];

        for root_expr in nodes {
            match root_expr.into_inner() {
                RootExpr::Expr(node_expr) => {
                    self.fallible_expression_error = None;

                    if let Some(expr) = self.compile_expr(node_expr, state) {
                        if let Some(error) = self.fallible_expression_error.take() {
                            self.diagnostics.push(error);
                        }

                        node_exprs.push(expr);
                    }
                }
                RootExpr::Error(err) => self.handle_parser_error(err),
            }
        }

        if node_exprs.is_empty() {
            node_exprs.push(Expr::Noop(Noop));
        }
        node_exprs
    }

    fn compile_block(&mut self, node: Node<ast::Block>, state: &mut TypeState) -> Option<Block> {
        self.compile_block_with_type(node, state)
            .map(|(block, _type_def)| block)
    }

    fn compile_block_with_type(
        &mut self,
        node: Node<ast::Block>,
        state: &mut TypeState,
    ) -> Option<(Block, TypeDef)> {
        let original_state = state.clone();
        let exprs = self.compile_exprs(node.into_inner().into_iter(), state)?;
        let block = Block::new_scoped(exprs);

        // The type information from `compile_exprs` doesn't applying the "scoping" from the block.
        // This is recalculated using the block.
        *state = original_state;
        let result = block.apply_type_info(state);
        Some((block, result))
    }

    fn compile_array(&mut self, node: Node<ast::Array>, state: &mut TypeState) -> Option<Array> {
        let exprs = self.compile_exprs(node.into_inner().into_iter(), state)?;

        Some(Array::new(exprs))
    }

    fn compile_object(&mut self, node: Node<ast::Object>, state: &mut TypeState) -> Option<Object> {
        use std::collections::BTreeMap;

        let (keys, exprs): (Vec<String>, Vec<Option<Expr>>) = node
            .into_inner()
            .into_iter()
            .map(|(k, expr)| (k.into_inner(), self.compile_expr(expr, state)))
            .unzip();

        let exprs = exprs.into_iter().collect::<Option<Vec<_>>>()?;

        Some(Object::new(
            keys.into_iter().zip(exprs).collect::<BTreeMap<_, _>>(),
        ))
    }

    #[cfg(feature = "expr-if_statement")]
    fn compile_if_statement(
        &mut self,
        node: Node<ast::IfStatement>,
        state: &mut TypeState,
    ) -> Option<IfStatement> {
        let ast::IfStatement {
            predicate,
            if_node,
            else_node,
        } = node.into_inner();

        let original_state = state.clone();

        let predicate = self
            .compile_predicate(predicate, state)?
            .map_err(|err| self.diagnostics.push(Box::new(err)))
            .ok()?;

        let after_predicate_state = state.clone();

        let if_block = self.compile_block(if_node, state)?;

        let else_block = if let Some(else_node) = else_node {
            *state = after_predicate_state;
            Some(self.compile_block(else_node, state)?)
        } else {
            None
        };

        let if_statement = IfStatement {
            predicate,
            if_block,
            else_block,
        };

        // The current state is from one of the branches. Restore it and calculate
        // the type state from the full "if statement" expression.
        *state = original_state;
        if_statement.apply_type_info(state);
        Some(if_statement)
    }

    #[cfg(not(feature = "expr-if_statement"))]
    fn compile_if_statement(
        &mut self,
        node: Node<ast::IfStatement>,
        _: &mut ExternalEnv,
    ) -> Option<Expr> {
        self.handle_missing_feature_error(node.span(), "expr-if_statement")
    }

    #[cfg(feature = "expr-if_statement")]
    fn compile_predicate(
        &mut self,
        node: Node<ast::Predicate>,
        state: &mut TypeState,
    ) -> Option<predicate::Result> {
        use ast::Predicate::{Many, One};

        let (span, predicate) = node.take();

        let exprs = match predicate {
            One(node) => vec![self.compile_expr(*node, state)?],
            Many(nodes) => self.compile_exprs(nodes, state)?,
        };

        Some(Predicate::new(
            Node::new(span, exprs),
            state,
            self.fallible_expression_error.as_deref(),
        ))
    }

    #[cfg(feature = "expr-op")]
    fn compile_op(&mut self, node: Node<ast::Op>, state: &mut TypeState) -> Option<Op> {
        use parser::ast::Opcode;

        let original_state = state.clone();

        let op = node.into_inner();
        let ast::Op(lhs, opcode, rhs) = op;

        let lhs_span = lhs.span();
        let lhs = Node::new(lhs_span, self.compile_expr(*lhs, state)?);

        // If we're using error-coalescing, we need to negate any tracked
        // fallibility error state for the lhs expression.
        if opcode.inner() == &Opcode::Err {
            self.fallible_expression_error = None;
        }

        // save the error so the RHS can't delete an error from the LHS
        let fallible_expression_error = self.fallible_expression_error.take();

        let rhs_span = rhs.span();
        let rhs = Node::new(rhs_span, self.compile_expr(*rhs, state)?);

        let op = Op::new(lhs, opcode, rhs, state)
            .map_err(|err| self.diagnostics.push(Box::new(err)))
            .ok()?;

        let type_info = op.type_info(&original_state);

        // re-apply the RHS error saved from above
        if self.fallible_expression_error.is_none() {
            self.fallible_expression_error = fallible_expression_error;
        }

        if type_info.result.is_infallible() {
            // There was a short-circuit operation that is preventing a fallibility error
            self.fallible_expression_error = None;
        }

        // Both "lhs" and "rhs" are compiled above, but "rhs" isn't always executed.
        // The expression can provide a more accurate type state.
        *state = type_info.state;
        Some(op)
    }

    #[cfg(not(feature = "expr-op"))]
    fn compile_op(&mut self, node: Node<ast::Op>, _: &mut ExternalEnv) -> Option<Expr> {
        self.handle_missing_feature_error(node.span(), "expr-op")
    }

    /// Rewrites the ast for `a |= b` to be `a = a | b`.
    #[cfg(feature = "expr-assignment")]
    fn rewrite_to_merge(
        &mut self,
        span: diagnostic::Span,
        target: &Node<ast::AssignmentTarget>,
        expr: Box<Node<ast::Expr>>,
        state: &mut TypeState,
    ) -> Option<Box<Node<Expr>>> {
        Some(Box::new(Node::new(
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
                state,
            )?),
        )))
    }

    #[cfg(feature = "expr-assignment")]
    fn compile_assignment(
        &mut self,
        node: Node<ast::Assignment>,
        state: &mut TypeState,
    ) -> Option<Assignment> {
        use assignment::Variant;
        use ast::{
            Assignment::{Infallible, Single},
            AssignmentOp,
        };

        let original_state = state.clone();

        let assignment = node.into_inner();

        let node = match assignment {
            Single { target, op, expr } => {
                let span = expr.span();

                match op {
                    AssignmentOp::Assign => {
                        let expr = self
                            .compile_expr(*expr, state)
                            .map(|expr| Box::new(Node::new(span, expr)))
                            .or_else(|| {
                                self.skip_missing_assignment_target(target.clone().into_inner());
                                None
                            })?;

                        Node::new(span, Variant::Single { target, expr })
                    }
                    AssignmentOp::Merge => {
                        let expr = self.rewrite_to_merge(span, &target, expr, state)?;
                        Node::new(span, Variant::Single { target, expr })
                    }
                }
            }
            Infallible { ok, err, op, expr } => {
                let span = expr.span();

                let node = match op {
                    AssignmentOp::Assign => {
                        let expr = self
                            .compile_expr(*expr, state)
                            .map(|expr| Box::new(Node::new(span, expr)))
                            .or_else(|| {
                                self.skip_missing_assignment_target(ok.clone().into_inner());
                                self.skip_missing_assignment_target(err.clone().into_inner());
                                None
                            })?;

                        let node = Variant::Infallible {
                            ok,
                            err,
                            expr,
                            default: Value::Null,
                        };
                        Node::new(span, node)
                    }
                    AssignmentOp::Merge => {
                        let expr = self.rewrite_to_merge(span, &ok, expr, state)?;
                        let node = Variant::Infallible {
                            ok,
                            err,
                            expr,
                            default: Value::Null,
                        };

                        Node::new(span, node)
                    }
                };

                // If the RHS expression is marked as fallible, the "infallible"
                // assignment nullifies this fallibility, and thus no error
                // should be emitted.
                self.fallible_expression_error = None;

                node
            }
        };

        let assignment = Assignment::new(
            node,
            state,
            self.fallible_expression_error.as_deref(),
            &self.config,
        )
        .map_err(|err| self.diagnostics.push(Box::new(err)))
        .ok()?;

        // Track any potential external target assignments within the program.
        //
        // This data is exposed to the caller of the compiler, to allow any
        // potential external optimizations.
        for target in assignment.targets() {
            if let assignment::Target::External(path) = target {
                self.external_assignments.push(path);
            }
        }

        // The state hasn't been updated from the actual assignment yet. Recalculate the type
        // from the new assignment expression.
        *state = original_state;
        assignment.apply_type_info(state);

        Some(assignment)
    }

    #[cfg(not(feature = "expr-assignment"))]
    fn compile_assignment(
        &mut self,
        node: Node<ast::Assignment>,
        _: &mut ExternalEnv,
    ) -> Option<Expr> {
        self.handle_missing_feature_error(node.span(), "expr-assignment")
    }

    #[cfg(feature = "expr-query")]
    fn compile_query(&mut self, node: Node<ast::Query>, state: &mut TypeState) -> Option<Query> {
        let ast::Query { target, path } = node.into_inner();

        if self
            .skip_missing_query_target
            .contains(&(target.clone().into_inner(), path.clone().into_inner()))
        {
            return None;
        }

        let path = path.into_inner();
        let target = self.compile_query_target(target, state)?;

        // Track any potential external target queries within the program.
        //
        // This data is exposed to the caller of the compiler, to allow any
        // potential external optimizations.
        if let Target::External(prefix) = target {
            let target_path = OwnedTargetPath {
                prefix,
                path: path.clone(),
            };
            self.external_queries.push(target_path);
        }

        Some(Query::new(target, path))
    }

    #[cfg(not(feature = "expr-query"))]
    fn compile_query(&mut self, node: Node<ast::Query>, _: &mut ExternalEnv) -> Option<Expr> {
        self.handle_missing_feature_error(node.span(), "expr-query")
    }

    #[cfg(feature = "expr-query")]
    fn compile_query_target(
        &mut self,
        node: Node<ast::QueryTarget>,
        state: &mut TypeState,
    ) -> Option<query::Target> {
        use ast::QueryTarget::{Container, External, FunctionCall, Internal};

        let span = node.span();

        let target = match node.into_inner() {
            External(prefix) => Target::External(prefix),
            Internal(ident) => {
                let variable = self.compile_variable(Node::new(span, ident), state)?;
                Target::Internal(variable)
            }
            Container(container) => {
                let container = self.compile_container(Node::new(span, container), state)?;
                Target::Container(container)
            }
            FunctionCall(call) => {
                let call = self.compile_function_call(Node::new(span, call), state)?;
                Target::FunctionCall(call)
            }
        };

        Some(target)
    }

    #[cfg(feature = "expr-function_call")]
    fn compile_function_call(
        &mut self,
        node: Node<ast::FunctionCall>,
        state: &mut TypeState,
    ) -> Option<FunctionCall> {
        let call_span = node.span();
        let ast::FunctionCall {
            ident,
            abort_on_error,
            arguments,
            closure,
        } = node.into_inner();

        let original_state = state.clone();
        // TODO: Remove this (hacky) code once dynamic path syntax lands.
        //
        // See: https://github.com/vectordotdev/vector/issues/12547
        if ident.as_deref() == "get" {
            self.external_queries.push(OwnedTargetPath::event_root());
        }

        let arguments: Vec<_> = arguments
            .into_iter()
            .map(|node| {
                Some(Node::new(
                    node.span(),
                    self.compile_function_argument(node, state)?,
                ))
            })
            .collect::<Option<_>>()?;

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
        let local_snapshot = state.local.clone();

        // TODO: The state passed into functions should be after function arguments
        //    have resolved, but this will break many functions relying on calling `type_def`
        //    on it's own args.
        // see: https://github.com/vectordotdev/vector/issues/13752
        let state_before_function = original_state.clone();

        // First, we create a new function-call builder to validate the
        // expression.
        let function_info = function_call::Builder::new(
            call_span,
            ident,
            abort_on_error,
            arguments,
            self.fns,
            &state_before_function,
            state,
            closure_variables,
        )
        // Then, we compile the closure block, and compile the final
        // function-call expression, including the attached closure.
        .map_err(|err| self.diagnostics.push(Box::new(err)))
        .ok()
        .and_then(|builder| {
            let block = match closure_block {
                None => None,
                Some(block) => {
                    let span = block.span();
                    match self.compile_block_with_type(block, state) {
                        Some(block_with_type) => Some(Node::new(span, block_with_type)),
                        None => return None,
                    }
                }
            };

            let arg_list = builder.get_arg_list().clone();

            builder
                .compile(
                    &state_before_function,
                    state,
                    block,
                    local_snapshot,
                    &mut self.fallible_expression_error,
                    &mut self.config,
                )
                .map_err(|err| self.diagnostics.push(Box::new(err)))
                .ok()
                .map(|func| (arg_list, func))
        });

        if let Some((_args, function)) = &function_info {
            // Update the final state using the function expression to make sure it's accurate.
            *state = function.type_info(&original_state).state;
        }

        function_info.map(|info| info.1)
    }

    #[cfg(feature = "expr-function_call")]
    fn compile_function_argument(
        &mut self,
        node: Node<ast::FunctionArgument>,
        state: &mut TypeState,
    ) -> Option<FunctionArgument> {
        let ast::FunctionArgument {
            ident,
            expr: ast_expr,
        } = node.into_inner();
        let span = ast_expr.span();
        let expr = self.compile_expr(ast_expr, state)?;
        let node = Node::new(span, expr);

        Some(FunctionArgument::new(ident, node))
    }

    #[cfg(not(feature = "expr-function_call"))]
    fn compile_function_call(
        &mut self,
        node: Node<ast::FunctionCall>,
        _: &mut ExternalEnv,
    ) -> Option<Noop> {
        // Guard against `dead_code` lint, to avoid having to sprinkle
        // attributes all over the place.
        let _ = self.fns;

        self.handle_missing_feature_error(node.span(), "expr-function_call");
        None
    }

    fn compile_variable(
        &mut self,
        node: Node<ast::Ident>,
        state: &mut TypeState,
    ) -> Option<Variable> {
        let (span, ident) = node.take();

        if self
            .skip_missing_query_target
            .contains(&(QueryTarget::Internal(ident.clone()), OwnedValuePath::root()))
        {
            return None;
        }

        Variable::new(span, ident, &state.local)
            .map_err(|err| self.diagnostics.push(Box::new(err)))
            .ok()
    }

    #[cfg(feature = "expr-unary")]
    fn compile_unary(&mut self, node: Node<ast::Unary>, state: &mut TypeState) -> Option<Unary> {
        use ast::Unary::Not;

        let variant = match node.into_inner() {
            Not(node) => self.compile_not(node, state)?.into(),
        };

        Some(Unary::new(variant))
    }

    #[cfg(not(feature = "expr-unary"))]
    fn compile_unary(&mut self, node: Node<ast::Unary>, _: &mut ExternalEnv) -> Option<Expr> {
        use ast::Unary::*;

        let span = match node.into_inner() {
            Not(node) => node.take().1.take().0,
        };

        self.handle_missing_feature_error(span.span(), "expr-unary")
    }

    #[cfg(feature = "expr-unary")]
    fn compile_not(&mut self, node: Node<ast::Not>, state: &mut TypeState) -> Option<Not> {
        let (not, expr) = node.into_inner().take();

        let node = Node::new(expr.span(), self.compile_expr(*expr, state)?);

        Not::new(node, not.span(), state)
            .map_err(|err| self.diagnostics.push(Box::new(err)))
            .ok()
    }

    #[cfg(feature = "expr-abort")]
    fn compile_abort(&mut self, node: Node<ast::Abort>, state: &mut TypeState) -> Option<Abort> {
        self.abortable = true;
        let (span, abort) = node.take();
        let message = match abort.message {
            Some(node) => {
                Some((*node).map_option(|expr| self.compile_expr(Node::new(span, expr), state))?)
            }
            None => None,
        };

        Abort::new(span, message, state)
            .map_err(|err| self.diagnostics.push(Box::new(err)))
            .ok()
    }

    #[cfg(not(feature = "expr-abort"))]
    fn compile_abort(&mut self, node: Node<ast::Abort>, _: &mut ExternalEnv) -> Option<Expr> {
        self.handle_missing_feature_error(node.span(), "expr-abort")
    }

    fn handle_parser_error(&mut self, error: parser::Error) {
        self.diagnostics.push(Box::new(error));
    }

    #[allow(dead_code)]
    fn handle_missing_feature_error(&mut self, span: Span, feature: &'static str) -> Option<Expr> {
        self.diagnostics
            .push(Box::new(Error::Missing { span, feature }));

        None
    }

    #[cfg(feature = "expr-assignment")]
    fn skip_missing_assignment_target(&mut self, target: ast::AssignmentTarget) {
        let query = match &target {
            ast::AssignmentTarget::Noop => return,
            ast::AssignmentTarget::Query(ast::Query { target, path }) => {
                (target.clone().into_inner(), path.clone().into_inner())
            }
            ast::AssignmentTarget::Internal(ident, path) => (
                QueryTarget::Internal(ident.clone()),
                path.clone().unwrap_or_else(OwnedValuePath::root),
            ),
            ast::AssignmentTarget::External(path) => {
                let prefix = path.as_ref().map_or(PathPrefix::Event, |x| x.prefix);
                let path = path.clone().map_or_else(OwnedValuePath::root, |x| x.path);
                (QueryTarget::External(prefix), path)
            }
        };

        self.skip_missing_query_target.push(query);
    }
}
