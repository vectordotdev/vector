use std::{convert::TryFrom, fmt};

use diagnostic::{DiagnosticMessage, Label, Note};
use lookup::LookupBuf;
use value::Value;

use crate::{
    expression::{Expr, Resolved},
    kind::DefaultValue,
    parser::{
        ast::{self, Ident},
        Node,
    },
    state::{ExternalEnv, LocalEnv},
    type_def::Details,
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
        fallible_rhs: Option<&dyn DiagnosticMessage>,
    ) -> Result<Self, Error> {
        let (_, variant) = node.take();

        let variant = match variant {
            Variant::Single { target, expr } => {
                let target_span = target.span();
                let expr_span = expr.span();
                let assignment_span = Span::new(target_span.start(), expr_span.start() - 1);
                let type_def = expr.type_def((local, external));

                // Fallible expressions require infallible assignment.
                if fallible_rhs.is_some() {
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
                verify_mutable(&target, external, target_span, assignment_span)?;
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
                verify_mutable(&ok, external, expr_span, ok_span)?;
                let type_def = type_def.infallible();
                let default_value = type_def.default_value();
                let value = expr.as_value();

                ok.insert_type_def(local, external, type_def, value);

                // "err" target is assigned `null` or a string containing the
                // error message.
                let err = Target::try_from(err.into_inner())?;
                verify_mutable(&err, external, expr_span, err_span)?;
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

fn verify_mutable(
    target: &Target,
    external: &ExternalEnv,
    expr_span: Span,
    assignment_span: Span,
) -> Result<(), Error> {
    match target {
        Target::External(lookup_buf) => {
            if external.is_read_only_event_path(lookup_buf) {
                Err(Error {
                    variant: ErrorVariant::ReadOnly,
                    expr_span,
                    assignment_span,
                })
            } else {
                Ok(())
            }
        }
        Target::Internal(_, _) | Target::Noop => Ok(()),
    }
}

impl Expression for Assignment {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.variant.resolve(ctx)
    }

    fn type_def(&self, state: (&LocalEnv, &ExternalEnv)) -> TypeDef {
        self.variant.type_def(state)
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
        use Variant::{Infallible, Single};

        match &self.variant {
            Single { target, expr } => write!(f, "{} = {}", target, expr),
            Infallible { ok, err, expr, .. } => write!(f, "{}, {} = {}", ok, err, expr),
        }
    }
}

impl fmt::Debug for Assignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Variant::{Infallible, Single};

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
pub(crate) enum Target {
    Noop,
    Internal(Ident, LookupBuf),
    External(LookupBuf),
}

impl Target {
    fn insert_type_def(
        &self,
        local: &mut LocalEnv,
        external: &mut ExternalEnv,
        new_type_def: TypeDef,
        value: Option<Value>,
    ) {
        match self {
            Self::Noop => {}
            Self::Internal(ident, path) => {
                let type_def = match local.variable(ident) {
                    None => {
                        if path.is_root() {
                            new_type_def
                        } else {
                            new_type_def.for_path(&path.to_lookup())
                        }
                    }
                    Some(&Details { ref type_def, .. }) => type_def
                        .clone()
                        .with_type_set_at_path(&path.to_lookup(), new_type_def),
                };

                let details = Details { type_def, value };
                local.insert_variable(ident.clone(), details);
            }

            Self::External(path) => {
                external.update_target(Details {
                    type_def: external
                        .target()
                        .type_def
                        .clone()
                        .with_type_set_at_path(&path.to_lookup(), new_type_def),
                    value,
                });
            }
        }
    }

    fn insert(&self, value: Value, ctx: &mut Context) {
        use Target::{External, Internal, Noop};

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
    fn emit_llvm_insert<'ctx>(
        &self,
        ctx: &mut crate::llvm::Context<'ctx>,
        result_ref: inkwell::values::PointerValue<'ctx>,
    ) {
        match self {
            Target::Noop => {}
            Target::Internal(ident, path) => {
                let variable_ref = ctx.get_or_insert_variable_ref(ident);

                // Get the provided path, or else insert into the variable
                // without any path appended and return early.
                if path.is_root() {
                    ctx.fns().vrl_resolved_clone.build_call(
                        ctx.builder(),
                        ctx.cast_resolved_ref_type(result_ref),
                        variable_ref,
                    );
                    return;
                }

                // Update existing variable using the provided path, or create a
                // new value in the store.
                let path_ref = ctx.into_lookup_buf_const_ref(path.clone());
                let vrl_expression_assignment_target_insert_internal_path = ctx
                    .fns()
                    .vrl_expression_assignment_target_insert_internal_path;
                vrl_expression_assignment_target_insert_internal_path.build_call(
                    ctx.builder(),
                    result_ref,
                    ctx.cast_lookup_buf_ref_type(path_ref),
                    variable_ref,
                );
            }
            Target::External(path) => {
                let path_ref = ctx.into_lookup_buf_const_ref(path.clone());

                let vrl_expression_assignment_target_insert_external =
                    ctx.fns().vrl_expression_assignment_target_insert_external;
                vrl_expression_assignment_target_insert_external.build_call(
                    ctx.builder(),
                    ctx.cast_resolved_ref_type(result_ref),
                    ctx.cast_lookup_buf_ref_type(path_ref),
                    ctx.context_ref(),
                );
            }
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Target::{External, Internal, Noop};

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
        use Target::{External, Internal, Noop};

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
        use Target::{External, Internal, Noop};

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
        use Variant::{Infallible, Single};

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
        use Variant::{Infallible, Single};

        match self {
            Single { expr, .. } => expr.type_def(state),
            Infallible { expr, .. } => expr.type_def(state).infallible(),
        }
    }

    #[cfg(feature = "llvm")]
    fn emit_llvm<'ctx>(
        &self,
        state: (&mut LocalEnv, &mut ExternalEnv),
        ctx: &mut crate::llvm::Context<'ctx>,
    ) -> Result<(), String> {
        match self {
            Variant::Single { target, expr } => {
                let begin_block = ctx.append_basic_block("assignment_single_begin");
                let end_block = ctx.append_basic_block("assignment_single_end");

                ctx.build_unconditional_branch(begin_block);
                ctx.position_at_end(begin_block);

                ctx.emit_llvm(expr.as_ref(), ctx.result_ref(), state, end_block, vec![])?;

                target.emit_llvm_insert(ctx, ctx.result_ref());

                ctx.build_unconditional_branch(end_block);
                ctx.position_at_end(end_block);
            }
            Variant::Infallible {
                ok,
                err,
                expr,
                default,
            } => {
                let begin_block = ctx.append_basic_block("assignment_infallible_begin");
                let end_block = ctx.append_basic_block("assignment_infallible_end");
                let is_ok_block = ctx.append_basic_block("assignment_infallible_is_ok");
                let is_err_block = ctx.append_basic_block("assignment_infallible_is_err");

                ctx.build_unconditional_branch(begin_block);
                ctx.position_at_end(begin_block);

                let result_ref = ctx.result_ref();

                let assignment_ref =
                    ctx.build_alloca_resolved_initialized("assignment_infallible_expression");

                ctx.emit_llvm(
                    expr.as_ref(),
                    assignment_ref,
                    state,
                    end_block,
                    vec![(assignment_ref.into(), ctx.fns().vrl_resolved_drop)],
                )?;

                let is_ok = ctx
                    .fns()
                    .vrl_resolved_is_ok
                    .build_call(ctx.builder(), assignment_ref)
                    .try_as_basic_value()
                    .left()
                    .expect("result is not a basic value")
                    .try_into()
                    .expect("result is not an int value");

                ctx.build_conditional_branch(is_ok, is_ok_block, is_err_block);

                ctx.position_at_end(is_ok_block);
                ok.emit_llvm_insert(ctx, assignment_ref);
                let error_temp_ref =
                    ctx.build_alloca_resolved_initialized("assignment_infallible_ok_err");
                ctx.fns()
                    .vrl_resolved_ok_null
                    .build_call(ctx.builder(), error_temp_ref);
                err.emit_llvm_insert(ctx, error_temp_ref);
                ctx.fns()
                    .vrl_resolved_drop
                    .build_call(ctx.builder(), error_temp_ref);
                ctx.fns()
                    .vrl_resolved_move
                    .build_call(ctx.builder(), assignment_ref, result_ref);
                ctx.build_unconditional_branch(end_block);

                ctx.position_at_end(is_err_block);
                let default_ref = ctx.into_resolved_const_ref(Ok(default.clone()));
                ok.emit_llvm_insert(ctx, default_ref);
                ctx.fns()
                    .vrl_resolved_err_into_ok
                    .build_call(ctx.builder(), assignment_ref);
                err.emit_llvm_insert(ctx, assignment_ref);
                ctx.fns()
                    .vrl_resolved_move
                    .build_call(ctx.builder(), assignment_ref, result_ref);
                ctx.build_unconditional_branch(end_block);

                ctx.position_at_end(end_block);
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
        use Variant::{Infallible, Single};

        match self {
            Single { target, expr } => write!(f, "{} = {}", target, expr),
            Infallible { ok, err, expr, .. } => write!(f, "{}, {} = {}", ok, err, expr),
        }
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug)]
pub(crate) struct Error {
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

    #[error("mutation of read-only value")]
    ReadOnly,
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
        use ErrorVariant::{
            FallibleAssignment, InfallibleAssignment, InvalidTarget, ReadOnly, UnnecessaryNoop,
        };

        match &self.variant {
            UnnecessaryNoop(..) => 640,
            FallibleAssignment(..) => 103,
            InfallibleAssignment(..) => 104,
            InvalidTarget(..) => 641,
            ReadOnly => 315,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use ErrorVariant::{
            FallibleAssignment, InfallibleAssignment, InvalidTarget, ReadOnly, UnnecessaryNoop,
        };

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
            ReadOnly => vec![Label::primary(
                "mutation of read-only value",
                self.assignment_span,
            )],
        }
    }

    fn notes(&self) -> Vec<Note> {
        use ErrorVariant::{FallibleAssignment, InfallibleAssignment};

        match &self.variant {
            FallibleAssignment(..) | InfallibleAssignment(..) => vec![Note::SeeErrorDocs],
            _ => vec![],
        }
    }
}
