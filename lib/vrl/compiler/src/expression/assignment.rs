use crate::expression::{Expr, Literal, Resolved};
use crate::parser::{
    ast::{self, Ident},
    Node,
};
use crate::vm::OpCode;
use crate::{Context, Expression, Span, State, TypeDef, Value};
use diagnostic::{DiagnosticError, Label, Note};
use lookup::LookupBuf;
use std::convert::TryFrom;
use std::fmt;

#[derive(Clone, PartialEq)]
pub struct Assignment {
    variant: Variant<Target, Expr>,
}

impl Assignment {
    pub(crate) fn new(
        node: Node<Variant<Node<ast::AssignmentTarget>, Node<Expr>>>,
        state: &mut State,
    ) -> Result<Self, Error> {
        let (span, variant) = node.take();

        let variant = match variant {
            Variant::Single { target, expr } => {
                let target_span = target.span();
                let expr_span = expr.span();
                let assignment_span = Span::new(target_span.start(), expr_span.start() - 1);
                let type_def = expr.type_def(state);

                // Fallible expressions require infallible assignment.
                if type_def.is_fallible() {
                    return Err(Error {
                        variant: ErrorVariant::FallibleAssignment(
                            target.to_string(),
                            expr.to_string(),
                        ),
                        span,
                        expr_span,
                        assignment_span,
                    });
                }

                // Single-target no-op assignments are useless.
                if matches!(target.as_ref(), ast::AssignmentTarget::Noop) {
                    return Err(Error {
                        variant: ErrorVariant::UnnecessaryNoop(target_span),
                        span,
                        expr_span,
                        assignment_span,
                    });
                }

                let expr = expr.into_inner();
                let target = Target::try_from(target.into_inner())?;
                let value = match &expr {
                    Expr::Literal(v) => Some(v.to_value()),
                    _ => None,
                };

                target.insert_type_def(state, type_def, value);

                Variant::Single {
                    target,
                    expr: Box::new(expr),
                }
            }

            Variant::Infallible { ok, err, expr, .. } => {
                let ok_span = ok.span();
                let err_span = err.span();
                let expr_span = expr.span();
                let assignment_span = Span::new(ok_span.start(), err_span.end());
                let type_def = expr.type_def(state);

                // Infallible expressions do not need fallible assignment.
                if type_def.is_infallible() {
                    return Err(Error {
                        variant: ErrorVariant::InfallibleAssignment(
                            ok.to_string(),
                            expr.to_string(),
                            ok_span,
                            err_span,
                        ),
                        span,
                        expr_span,
                        assignment_span,
                    });
                }

                let ok_noop = matches!(ok.as_ref(), ast::AssignmentTarget::Noop);
                let err_noop = matches!(err.as_ref(), ast::AssignmentTarget::Noop);

                // Infallible-target no-op assignments are useless.
                if ok_noop && err_noop {
                    return Err(Error {
                        variant: ErrorVariant::UnnecessaryNoop(ok_span),
                        span,
                        expr_span,
                        assignment_span,
                    });
                }

                let expr = expr.into_inner();

                // "ok" target takes on the type definition of the value, but is
                // set to being infallible, as the error will be captured by the
                // "err" target.
                let ok = Target::try_from(ok.into_inner())?;
                let type_def = type_def.infallible();
                let default = type_def.kind().default_value();
                let value = match &expr {
                    Expr::Literal(v) => Some(v.to_value()),
                    _ => None,
                };

                ok.insert_type_def(state, type_def, value);

                // "err" target is assigned `null` or a string containing the
                // error message.
                let err = Target::try_from(err.into_inner())?;
                let type_def = TypeDef::new().bytes().add_null().infallible();

                err.insert_type_def(state, type_def, None);

                Variant::Infallible {
                    ok,
                    err,
                    expr: Box::new(expr),
                    default,
                }
            }
        };

        Ok(Self { variant })
    }

    pub(crate) fn noop() -> Self {
        let target = Target::Noop;
        let expr = Box::new(Expr::Literal(Literal::Null));
        let variant = Variant::Single { target, expr };

        Self { variant }
    }
}

impl Expression for Assignment {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.variant.resolve(ctx)
    }

    fn type_def(&self, state: &State) -> TypeDef {
        self.variant.type_def(state)
    }

    fn dump(&self, vm: &mut crate::vm::Vm) -> Result<(), String> {
        self.variant.dump(vm)
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: &crate::state::Compiler,
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> Result<(), String> {
        self.variant.emit_llvm(state, ctx)
    }
}

impl fmt::Display for Assignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Variant::*;

        match &self.variant {
            Single { target, expr } => write!(f, "{} = {}", target, expr),
            Infallible { ok, err, expr, .. } => write!(f, "{}, {} = {}", ok, err, expr),
        }
    }
}

impl fmt::Debug for Assignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Variant::*;

        match &self.variant {
            Single { target, expr } => write!(f, "{:?} = {:?}", target, expr),
            Infallible { ok, err, expr, .. } => {
                write!(f, "Ok({:?}), Err({:?}) = {:?}", ok, err, expr)
            }
        }
    }
}

// -----------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Target {
    Noop,
    Internal(Ident, Option<LookupBuf>),
    External(Option<LookupBuf>),
}

impl Target {
    fn insert_type_def(&self, state: &mut State, type_def: TypeDef, value: Option<Value>) {
        use Target::*;

        fn set_type_def(
            current_type_def: &TypeDef,
            new_type_def: TypeDef,
            path: &Option<LookupBuf>,
        ) -> TypeDef {
            // If the assignment is onto root or has no path (root variable assignment), use the
            // new type def, otherwise merge the type defs.
            if path.as_ref().map(|path| path.is_root()).unwrap_or(true) {
                new_type_def
            } else {
                current_type_def.clone().merge_overwrite(new_type_def)
            }
        }

        match self {
            Noop => {}
            Internal(ident, path) => {
                let td = match path {
                    None => type_def,
                    Some(path) => type_def.for_path(path.clone()),
                };

                let type_def = match state.variable(ident) {
                    None => td,
                    Some(&Details { ref type_def, .. }) => set_type_def(type_def, td, path),
                };

                let details = Details { type_def, value };

                state.insert_variable(ident.clone(), details);
            }

            External(path) => {
                let td = match path {
                    None => type_def,
                    Some(path) => type_def.for_path(path.clone()),
                };

                let type_def = match state.target() {
                    None => td,
                    Some(&Details { ref type_def, .. }) => set_type_def(type_def, td, path),
                };

                let details = Details { type_def, value };

                state.update_target(details);
            }
        }
    }

    pub fn insert(&self, value: Value, ctx: &mut Context) {
        use Target::*;

        match self {
            Noop => {}
            Internal(ident, path) => {
                // Get the provided path, or else insert into the variable
                // without any path appended and return early.
                let path = match path {
                    Some(path) => path,
                    None => return ctx.state_mut().insert_variable(ident.clone(), value),
                };

                // Update existing variable using the provided path, or create a
                // new value in the store.
                match ctx.state_mut().variable_mut(ident) {
                    Some(stored) => stored.insert_by_path(path, value),
                    None => ctx
                        .state_mut()
                        .insert_variable(ident.clone(), value.at_path(path)),
                }
            }

            External(path) => {
                let _ = ctx
                    .target_mut()
                    .insert(path.as_ref().unwrap_or(&LookupBuf::root()), value);
            }
        }
    }

    #[cfg(feature = "llvm")]
    pub fn emit_llvm_insert<'ctx>(
        &self,
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> Result<(), String> {
        match self {
            Target::Noop => Ok(()),
            Target::Internal(ident, path) => {
                let ident_ref = ctx.into_ident_const_ref(ident.clone());

                // Get the provided path, or else insert into the variable
                // without any path appended and return early.
                let path = match path {
                    Some(path) => path,
                    None => {
                        let fn_ident = "vrl_expression_assignment_target_insert_internal_impl";
                        let fn_impl = ctx
                            .module()
                            .get_function(fn_ident)
                            .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
                        ctx.builder().build_call(
                            fn_impl,
                            &[
                                ctx.context_ref().into(),
                                ctx.builder()
                                    .build_bitcast(
                                        ident_ref,
                                        fn_impl
                                            .get_nth_param(1)
                                            .unwrap()
                                            .get_type()
                                            .into_pointer_type(),
                                        "cast",
                                    )
                                    .into(),
                                ctx.result_ref().into(),
                            ],
                            fn_ident,
                        );
                        return Ok(());
                    }
                };

                // Update existing variable using the provided path, or create a
                // new value in the store.
                let path_ref = ctx.into_lookup_buf_const_ref(path.clone());
                let fn_ident = "vrl_expression_assignment_target_insert_internal_path_impl";
                let fn_impl = ctx
                    .module()
                    .get_function(fn_ident)
                    .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
                ctx.builder().build_call(
                    fn_impl,
                    &[
                        ctx.context_ref().into(),
                        ctx.builder()
                            .build_bitcast(
                                ident_ref,
                                fn_impl
                                    .get_nth_param(1)
                                    .unwrap()
                                    .get_type()
                                    .into_pointer_type(),
                                "cast",
                            )
                            .into(),
                        ctx.builder()
                            .build_bitcast(
                                path_ref,
                                fn_impl
                                    .get_nth_param(2)
                                    .unwrap()
                                    .get_type()
                                    .into_pointer_type(),
                                "cast",
                            )
                            .into(),
                        ctx.result_ref().into(),
                    ],
                    fn_ident,
                );
                Ok(())
            }
            Target::External(path) => {
                let path_ref =
                    ctx.into_lookup_buf_const_ref(path.clone().unwrap_or(LookupBuf::root()));

                let fn_ident = "vrl_expression_assignment_target_insert_external_impl";
                let fn_impl = ctx
                    .module()
                    .get_function(fn_ident)
                    .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
                ctx.builder().build_call(
                    fn_impl,
                    &[
                        ctx.context_ref().into(),
                        ctx.builder()
                            .build_bitcast(
                                path_ref,
                                fn_impl
                                    .get_nth_param(1)
                                    .unwrap()
                                    .get_type()
                                    .into_pointer_type(),
                                "cast",
                            )
                            .into(),
                        ctx.result_ref().into(),
                    ],
                    fn_ident,
                );
                Ok(())
            }
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Target::*;

        match self {
            Noop => f.write_str("_"),
            Internal(ident, Some(path)) => write!(f, "{}{}", ident, path),
            Internal(ident, None) => ident.fmt(f),
            External(Some(path)) => write!(f, ".{}", path),
            External(None) => f.write_str("."),
        }
    }
}

impl fmt::Debug for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Target::*;

        match self {
            Noop => f.write_str("Noop"),
            Internal(ident, Some(path)) => write!(f, "Internal({}{})", ident, path),
            Internal(ident, _) => write!(f, "Internal({})", ident),
            External(Some(path)) => write!(f, "External({})", path),
            External(_) => f.write_str("External(.)"),
        }
    }
}

impl TryFrom<ast::AssignmentTarget> for Target {
    type Error = Error;

    fn try_from(target: ast::AssignmentTarget) -> Result<Self, Error> {
        use Target::*;

        let target = match target {
            ast::AssignmentTarget::Noop => Noop,
            ast::AssignmentTarget::Query(query) => {
                let ast::Query { target, path } = query;

                let (target_span, target) = target.take();
                let (path_span, path) = path.take();

                let span = Span::new(target_span.start(), path_span.end());

                match target {
                    ast::QueryTarget::Internal(ident) => Internal(ident, Some(path)),
                    ast::QueryTarget::External => External(Some(path)),
                    _ => {
                        return Err(Error {
                            variant: ErrorVariant::InvalidTarget(span),
                            span,
                            expr_span: span,
                            assignment_span: span,
                        })
                    }
                }
            }
            ast::AssignmentTarget::Internal(ident, path) => Internal(ident, path.map(Into::into)),
            ast::AssignmentTarget::External(path) => External(path.map(Into::into)),
        };

        Ok(target)
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Variant<T, U> {
    Single {
        target: T,
        expr: Box<U>,
    },
    Infallible {
        ok: T,
        err: T,
        expr: Box<U>,

        /// The default `ok` value used when the expression results in an error.
        default: Value,
    },
}

impl<U> Expression for Variant<Target, U>
where
    U: Expression + Clone,
{
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Variant::*;

        let value = match self {
            Single { target, expr } => {
                let value = expr.resolve(ctx)?;
                target.insert(value.clone(), ctx);
                value
            }
            Infallible {
                ok,
                err,
                expr,
                default,
            } => match expr.resolve(ctx) {
                Ok(value) => {
                    ok.insert(value.clone(), ctx);
                    err.insert(Value::Null, ctx);
                    value
                }
                Err(error) => {
                    ok.insert(default.clone(), ctx);
                    let value = Value::from(error.to_string());
                    err.insert(value.clone(), ctx);
                    value
                }
            },
        };

        Ok(value)
    }

    fn type_def(&self, state: &State) -> TypeDef {
        use Variant::*;

        match self {
            Single { expr, .. } => expr.type_def(state),
            Infallible { expr, .. } => expr.type_def(state).infallible(),
        }
    }

    fn dump(&self, vm: &mut crate::vm::Vm) -> Result<(), String> {
        match self {
            Variant::Single { target, expr } => {
                expr.dump(vm)?;
                vm.write_chunk(OpCode::SetPath);

                let variable = match target {
                    Target::External(Some(path)) => crate::vm::Variable::External(path.clone()),
                    _ => unimplemented!("nothing else supported"),
                };

                let target = vm.get_target(&variable);
                vm.write_primitive(target);
            }
            Variant::Infallible {
                ok,
                err: _,
                expr,
                default: _,
            } => {
                // This isn't handling the error case yet.
                expr.dump(vm)?;
                vm.write_chunk(OpCode::SetPath);

                let variable = match ok {
                    Target::External(Some(path)) => crate::vm::Variable::External(path.clone()),
                    _ => unimplemented!("nothing else supported"),
                };

                let target = vm.get_target(&variable);
                vm.write_primitive(target);
            }
        }
        Ok(())
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: &crate::state::Compiler,
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> Result<(), String> {
        Ok(match self {
            Variant::Single { target, expr } => {
                let function = ctx.function();
                let assignment_single_begin_block = ctx
                    .context()
                    .append_basic_block(function, "assignment_single_begin");
                ctx.builder()
                    .build_unconditional_branch(assignment_single_begin_block);
                ctx.builder().position_at_end(assignment_single_begin_block);

                expr.emit_llvm(state, ctx)?;

                target.emit_llvm_insert(ctx)?;
            }
            Variant::Infallible {
                ok,
                err,
                expr,
                default,
            } => {
                let function = ctx.function();
                let assignment_infallible_begin_block = ctx
                    .context()
                    .append_basic_block(function, "assignment_infallible_begin");
                ctx.builder()
                    .build_unconditional_branch(assignment_infallible_begin_block);
                ctx.builder()
                    .position_at_end(assignment_infallible_begin_block);

                expr.emit_llvm(state, ctx)?;

                let is_ok = {
                    let fn_ident = "vrl_resolved_is_ok";
                    let fn_impl = ctx
                        .module()
                        .get_function(fn_ident)
                        .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
                    ctx.builder()
                        .build_call(fn_impl, &[ctx.result_ref().into()], fn_ident)
                        .try_as_basic_value()
                        .left()
                        .ok_or(format!(r#"result of "{}" is not a basic value"#, fn_ident))?
                        .try_into()
                        .map_err(|_| format!(r#"result of "{}" is not an int value"#, fn_ident))?
                };

                let assignment_infallible_end_block = ctx
                    .context()
                    .append_basic_block(function, "assignment_infallible_end");
                let assignment_infallible_begin_is_ok_block = ctx
                    .context()
                    .append_basic_block(function, "assignment_infallible_begin_is_ok");
                let assignment_infallible_begin_is_err_block = ctx
                    .context()
                    .append_basic_block(function, "assignment_infallible_begin_is_err");

                ctx.builder().build_conditional_branch(
                    is_ok,
                    assignment_infallible_begin_is_ok_block,
                    assignment_infallible_begin_is_err_block,
                );

                ctx.builder()
                    .position_at_end(assignment_infallible_begin_is_ok_block);

                ok.emit_llvm_insert(ctx)?;

                let result_ref = ctx.result_ref();
                let result_temp_ref = ctx.build_alloca_resolved("temp")?;
                ctx.set_result_ref(result_temp_ref);

                {
                    let fn_ident = "vrl_resolved_initialize";
                    let fn_impl = ctx
                        .module()
                        .get_function(fn_ident)
                        .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
                    ctx.builder()
                        .build_call(fn_impl, &[result_temp_ref.into()], fn_ident);
                }

                err.emit_llvm_insert(ctx)?;

                {
                    let fn_ident = "vrl_resolved_drop";
                    let fn_impl = ctx
                        .module()
                        .get_function(fn_ident)
                        .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
                    ctx.builder()
                        .build_call(fn_impl, &[result_temp_ref.into()], fn_ident);
                }

                ctx.set_result_ref(result_ref);

                ctx.builder()
                    .build_unconditional_branch(assignment_infallible_end_block);

                ctx.builder()
                    .position_at_end(assignment_infallible_begin_is_err_block);

                let default_ref = ctx.into_resolved_const_ref(Ok(default.clone()));
                ctx.set_result_ref(default_ref);

                ok.emit_llvm_insert(ctx)?;

                ctx.set_result_ref(result_ref);

                {
                    let fn_ident = "vrl_resolved_err_into_ok";
                    let fn_impl = ctx
                        .module()
                        .get_function(fn_ident)
                        .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
                    ctx.builder()
                        .build_call(fn_impl, &[ctx.result_ref().into()], fn_ident);
                }

                err.emit_llvm_insert(ctx)?;

                ctx.builder()
                    .build_unconditional_branch(assignment_infallible_end_block);

                ctx.builder()
                    .position_at_end(assignment_infallible_end_block);
            }
        })
    }
}

impl<T, U> fmt::Display for Variant<T, U>
where
    T: fmt::Display,
    U: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Variant::*;

        match self {
            Single { target, expr } => write!(f, "{} = {}", target, expr),
            Infallible { ok, err, expr, .. } => write!(f, "{}, {} = {}", ok, err, expr),
        }
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct Details {
    pub type_def: TypeDef,
    pub value: Option<Value>,
}

// -----------------------------------------------------------------------------

#[derive(Debug)]
pub struct Error {
    variant: ErrorVariant,
    span: Span,
    expr_span: Span,
    assignment_span: Span,
}

#[derive(thiserror::Error, Debug)]
pub enum ErrorVariant {
    #[error("unnecessary no-op assignment")]
    UnnecessaryNoop(Span),

    #[error("unhandled fallible assignment")]
    FallibleAssignment(String, String),

    #[error("unnecessary error assignment")]
    InfallibleAssignment(String, String, Span, Span),

    #[error("invalid assignment target")]
    InvalidTarget(Span),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#}", self.variant)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.variant)
    }
}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        use ErrorVariant::*;

        match &self.variant {
            UnnecessaryNoop(..) => 640,
            FallibleAssignment(..) => 103,
            InfallibleAssignment(..) => 104,
            InvalidTarget(..) => 641,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use ErrorVariant::*;

        match &self.variant {
            UnnecessaryNoop(target_span) => vec![
                Label::primary("this no-op assignment has no effect", self.expr_span),
                Label::context("either assign to a path or variable here", *target_span),
                Label::context("or remove the assignment", self.assignment_span),
            ],
            FallibleAssignment(target, expr) => vec![
                Label::primary("this expression is fallible", self.expr_span),
                Label::context("update the expression to be infallible", self.expr_span),
                Label::context(
                    "or change this to an infallible assignment:",
                    self.assignment_span,
                ),
                Label::context(format!("{}, err = {}", target, expr), self.assignment_span),
            ],
            InfallibleAssignment(target, expr, ok_span, err_span) => vec![
                Label::primary("this error assignment is unnecessary", err_span),
                Label::context("because this expression can't fail", self.expr_span),
                Label::context(format!("use: {} = {}", target, expr), ok_span),
            ],
            InvalidTarget(span) => vec![
                Label::primary("invalid assignment target", span),
                Label::context("use one of variable or path", span),
            ],
        }
    }

    fn notes(&self) -> Vec<Note> {
        use ErrorVariant::*;

        match &self.variant {
            FallibleAssignment(..) | InfallibleAssignment(..) => vec![Note::SeeErrorDocs],
            _ => vec![],
        }
    }
}
