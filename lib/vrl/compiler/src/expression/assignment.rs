use std::{convert::TryFrom, fmt};

use diagnostic::{DiagnosticMessage, Label, Note};
use lookup::LookupBuf;
use value::Value;

use crate::{
    expression::{Expr, Noop, Resolved},
    parser::{
        ast::{self, Ident},
        Node,
    },
    state::{ExternalEnv, LocalEnv},
    type_def::Details,
    value::kind::DefaultValue,
    vm::OpCode,
    Context, Expression, Span, TypeDef,
};

#[derive(Clone, PartialEq)]
pub struct Assignment {
    variant: Variant<Target, Expr>,
}

impl Assignment {
    pub(crate) fn new(
        node: Node<Variant<Node<ast::AssignmentTarget>, Node<Expr>>>,
        local: &mut LocalEnv,
        external: &mut ExternalEnv,
    ) -> Result<Self, Error> {
        let (_, variant) = node.take();

        let variant = match variant {
            Variant::Single { target, expr } => {
                let target_span = target.span();
                let expr_span = expr.span();
                let assignment_span = Span::new(target_span.start(), expr_span.start() - 1);
                let type_def = expr.type_def((local, external));

                // Fallible expressions require infallible assignment.
                if type_def.is_fallible() {
                    return Err(Error {
                        variant: ErrorVariant::FallibleAssignment(
                            target.to_string(),
                            expr.to_string(),
                        ),
                        expr_span,
                        assignment_span,
                    });
                }

                // Single-target no-op assignments are useless.
                if matches!(target.as_ref(), ast::AssignmentTarget::Noop) {
                    return Err(Error {
                        variant: ErrorVariant::UnnecessaryNoop(target_span),
                        expr_span,
                        assignment_span,
                    });
                }

                let expr = expr.into_inner();
                let target = Target::try_from(target.into_inner())?;
                let value = expr.as_value();

                target.insert_type_def(local, external, type_def, value);

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
                let type_def = expr.type_def((local, external));

                // Infallible expressions do not need fallible assignment.
                if type_def.is_infallible() {
                    return Err(Error {
                        variant: ErrorVariant::InfallibleAssignment(
                            ok.to_string(),
                            expr.to_string(),
                            ok_span,
                            err_span,
                        ),
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
                let default_value = type_def.default_value();
                let value = expr.as_value();

                ok.insert_type_def(local, external, type_def, value);

                // "err" target is assigned `null` or a string containing the
                // error message.
                let err = Target::try_from(err.into_inner())?;
                let type_def = TypeDef::bytes().add_null().infallible();

                err.insert_type_def(local, external, type_def, None);

                Variant::Infallible {
                    ok,
                    err,
                    expr: Box::new(expr),
                    default: default_value,
                }
            }
        };

        Ok(Self { variant })
    }

    pub(crate) fn noop() -> Self {
        let target = Target::Noop;
        let expr = Box::new(Expr::Noop(Noop));
        let variant = Variant::Single { target, expr };

        Self { variant }
    }

    /// Get a list of targets for this assignment.
    ///
    /// For regular assignments, this contains a single target, for infallible
    /// assignments, it'll contain both the `ok` and `err` target.
    pub(crate) fn targets(&self) -> Vec<Target> {
        let mut targets = Vec::with_capacity(2);

        match &self.variant {
            Variant::Single { target, .. } => targets.push(target.clone()),
            Variant::Infallible { ok, err, .. } => {
                targets.push(ok.clone());
                targets.push(err.clone());
            }
        }

        targets
    }
}

impl Expression for Assignment {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.variant.resolve(ctx)
    }

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        self.variant.type_def(state)
    }

    fn compile_to_vm(
        &self,
        vm: &mut crate::vm::Vm,
        state: (&mut LocalEnv, &mut ExternalEnv),
    ) -> Result<(), String> {
        self.variant.compile_to_vm(vm, state)
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: (&mut LocalEnv, &mut ExternalEnv),
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
    Internal(Ident, LookupBuf),
    External(LookupBuf),
}

impl Target {
    fn insert_type_def(
        &self,
        local: &mut LocalEnv,
        external: &mut ExternalEnv,
        type_def: TypeDef,
        value: Option<Value>,
    ) {
        use Target::*;

        fn set_type_def(
            current_type_def: &TypeDef,
            new_type_def: TypeDef,
            path: &LookupBuf,
        ) -> TypeDef {
            // If the assignment is onto root or has no path (root variable assignment), use the
            // new type def, otherwise merge the type defs.
            if path.is_root() {
                new_type_def
            } else {
                current_type_def.clone().merge_overwrite(new_type_def)
            }
        }

        match self {
            Noop => {}
            Internal(ident, path) => {
                let td = match path.is_root() {
                    true => type_def,
                    false => type_def.for_path(&path.to_lookup()),
                };

                let type_def = match local.variable(ident) {
                    None => td,
                    Some(&Details { ref type_def, .. }) => set_type_def(type_def, td, path),
                };

                let details = Details { type_def, value };

                local.insert_variable(ident.clone(), details);
            }

            External(path) => {
                let td = match path.is_root() {
                    true => type_def,
                    false => type_def.for_path(&path.to_lookup()),
                };

                let type_def = match external.target() {
                    None => td,
                    Some(&Details { ref type_def, .. }) => set_type_def(type_def, td, path),
                };

                let details = Details { type_def, value };

                external.update_target(details);
            }
        }
    }

    fn insert(&self, value: Value, ctx: &mut Context) {
        use Target::*;

        match self {
            Noop => {}
            Internal(ident, path) => {
                // Get the provided path, or else insert into the variable
                // without any path appended and return early.
                let path = match path.is_root() {
                    false => path,
                    true => return ctx.state_mut().insert_variable(ident.clone(), value),
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
                let _ = ctx.target_mut().target_insert(path, value);
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
                let variable_ref = ctx.get_or_insert_variable_ref(ident);

                // Get the provided path, or else insert into the variable
                // without any path appended and return early.
                if path.is_root() {
                    let fn_ident = "vrl_target_assign";
                    let fn_impl = ctx
                        .module()
                        .get_function(fn_ident)
                        .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
                    ctx.builder().build_call(
                        fn_impl,
                        &[ctx.result_ref().into(), variable_ref.into()],
                        fn_ident,
                    );
                    return Ok(());
                }

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
                        ctx.result_ref().into(),
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
                        variable_ref.into(),
                    ],
                    fn_ident,
                );
                Ok(())
            }
            Target::External(path) => {
                let path_ref = ctx.into_lookup_buf_const_ref(path.clone());

                let fn_ident = "vrl_expression_assignment_target_insert_external_impl";
                let fn_impl = ctx
                    .module()
                    .get_function(fn_ident)
                    .ok_or(format!(r#"failed to get "{}" function"#, fn_ident))?;
                ctx.builder().build_call(
                    fn_impl,
                    &[
                        ctx.builder()
                            .build_bitcast(
                                ctx.result_ref(),
                                fn_impl
                                    .get_nth_param(0)
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
                                    .get_nth_param(1)
                                    .unwrap()
                                    .get_type()
                                    .into_pointer_type(),
                                "cast",
                            )
                            .into(),
                        ctx.context_ref().into(),
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
            Internal(ident, path) if path.is_root() => ident.fmt(f),
            Internal(ident, path) => write!(f, "{}{}", ident, path),
            External(path) if path.is_root() => f.write_str("."),
            External(path) => write!(f, ".{}", path),
        }
    }
}

impl fmt::Debug for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Target::*;

        match self {
            Noop => f.write_str("Noop"),
            Internal(ident, path) if path.is_root() => write!(f, "Internal({})", ident),
            Internal(ident, path) => write!(f, "Internal({}{})", ident, path),
            External(path) if path.is_root() => f.write_str("External(.)"),
            External(path) => write!(f, "External({})", path),
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
                    ast::QueryTarget::Internal(ident) => Internal(ident, path),
                    ast::QueryTarget::External => External(path),
                    _ => {
                        return Err(Error {
                            variant: ErrorVariant::InvalidTarget(span),
                            expr_span: span,
                            assignment_span: span,
                        })
                    }
                }
            }
            ast::AssignmentTarget::Internal(ident, path) => {
                Internal(ident, path.unwrap_or_else(LookupBuf::root))
            }
            ast::AssignmentTarget::External(path) => External(path.unwrap_or_else(LookupBuf::root)),
        };

        Ok(target)
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Variant<T, U> {
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

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        use Variant::*;

        match self {
            Single { expr, .. } => expr.type_def(state),
            Infallible { expr, .. } => expr.type_def(state).infallible(),
        }
    }

    fn compile_to_vm(
        &self,
        vm: &mut crate::vm::Vm,
        state: (&mut LocalEnv, &mut ExternalEnv),
    ) -> Result<(), String> {
        match self {
            Variant::Single { target, expr } => {
                // Compile the expression which will leave the result at the top of the stack.
                expr.compile_to_vm(vm, state)?;

                vm.write_opcode(OpCode::SetPath);

                // Add the target to the list of targets, write its index as a primitive for the
                //  `SetPath` opcode to retrieve.
                let target = vm.get_target(&target.into());
                vm.write_primitive(target);
            }
            Variant::Infallible {
                ok,
                err,
                expr,
                default,
            } => {
                // Compile the expression which will leave the result at the top of the stack.
                expr.compile_to_vm(vm, state)?;
                vm.write_opcode(OpCode::SetPathInfallible);

                // Write the target for the `Ok` path.
                let target = vm.get_target(&ok.into());
                vm.write_primitive(target);

                // Write the target for the `Error` path.
                let target = vm.get_target(&err.into());
                vm.write_primitive(target);

                // Add the default value (the value to set to the `Ok` target should we have an error).
                let default = vm.add_constant(default.clone());
                vm.write_primitive(default);
            }
        }
        Ok(())
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: (&mut LocalEnv, &mut ExternalEnv),
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> Result<(), String> {
        match self {
            Variant::Single { target, expr } => {
                let function = ctx.function();
                let assignment_single_begin_block = ctx
                    .context()
                    .append_basic_block(function, "assignment_single_begin");
                ctx.builder()
                    .build_unconditional_branch(assignment_single_begin_block);
                ctx.builder().position_at_end(assignment_single_begin_block);

                let assignment_single_end_block = ctx
                    .context()
                    .append_basic_block(function, "assignment_single_end");

                expr.emit_llvm((state.0, state.1), ctx)?;

                if expr.type_def((state.0, state.1)).is_abortable() {
                    let is_err = {
                        let fn_ident = "vrl_resolved_is_err";
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
                            .map_err(|_| {
                                format!(r#"result of "{}" is not an int value"#, fn_ident)
                            })?
                    };

                    let assignment_single_is_ok_block = ctx
                        .context()
                        .append_basic_block(function, "assignment_single_is_ok");

                    ctx.builder().build_conditional_branch(
                        is_err,
                        assignment_single_end_block,
                        assignment_single_is_ok_block,
                    );

                    ctx.builder().position_at_end(assignment_single_is_ok_block);
                }

                target.emit_llvm_insert(ctx)?;

                ctx.builder()
                    .build_unconditional_branch(assignment_single_end_block);
                ctx.builder().position_at_end(assignment_single_end_block);
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
                let result_temp_ref = ctx.build_alloca_resolved("temp");
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
        }

        Ok(())
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

#[derive(Debug)]
pub struct Error {
    variant: ErrorVariant,
    expr_span: Span,
    assignment_span: Span,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum ErrorVariant {
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

impl DiagnosticMessage for Error {
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
