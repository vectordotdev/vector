use crate::expression::{Expr, Literal, Resolved};
use crate::parser::{
    ast::{self, Ident},
    Node,
};
use crate::{value::Kind, Context, Expression, Path, Span, State, TypeDef, Value};
use diagnostic::{DiagnosticError, Label, Note};
use std::fmt;

#[derive(PartialEq)]
pub struct Assignment {
    variant: Variant<Target, Expr>,
}

impl Assignment {
    pub(crate) fn new(
        node: Node<Variant<Node<ast::AssignmentTarget>, Node<Expr>>>,
        state: &mut State,
    ) -> Result<Self, Error> {
        let (span, variant) = node.take();

        match variant {
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
                        variant: ErrorVariant::UnneededNoop(target_span),
                        span,
                        expr_span,
                        assignment_span,
                    });
                }

                let target = Target::from(target.into_inner());

                state.insert_assignment(target.clone(), type_def);

                let variant = Variant::Single {
                    target,
                    expr: Box::new(expr.into_inner()),
                };

                Ok(Self { variant })
            }

            Variant::Infallible { ok, err, expr } => {
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
                        variant: ErrorVariant::UnneededNoop(ok_span),
                        span,
                        expr_span,
                        assignment_span,
                    });
                }

                // "ok" target takes on the type definition of the value, but is
                // set to being infallible, as the error will be captured by the
                // "err" target.
                let type_def = type_def.fallible();

                // "err" target is assigned `null` or a string containing the
                // error message.
                let err_type_def = TypeDef::new().scalar(Kind::Bytes | Kind::Null);

                let ok = Target::from(ok.into_inner());
                let err = Target::from(err.into_inner());

                state.insert_assignment(ok.clone(), type_def);
                state.insert_assignment(ok.clone(), err_type_def);

                let variant = Variant::Infallible {
                    ok,
                    err,
                    expr: Box::new(expr.into_inner()),
                };

                Ok(Self { variant })
            }
        }
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
}

impl fmt::Display for Assignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Variant::*;

        match &self.variant {
            Single { target, expr } => write!(f, "{} = {}", target, expr),
            Infallible { ok, err, expr } => write!(f, "{}, {} = {}", ok, err, expr),
        }
    }
}

impl fmt::Debug for Assignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Variant::*;

        match &self.variant {
            Single { target, expr } => write!(f, "{:?} = {:?}", target, expr),
            Infallible { ok, err, expr } => write!(f, "Ok({:?}), Err({:?}) = {:?}", ok, err, expr),
        }
    }
}

// -----------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Target {
    Noop,
    Internal(Ident, Option<Path>),
    External(Option<Path>),
}

impl Target {
    fn insert(&self, value: Value, ctx: &mut Context) {
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
                    Some(stored) => return stored.insert_by_path(path, value),
                    None => ctx
                        .state_mut()
                        .insert_variable(ident.clone(), value.at_path(path)),
                }
            }

            External(path) => {
                let _ = ctx
                    .target_mut()
                    .insert(path.as_ref().unwrap_or(&Path::root()), value);
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
            Internal(ident, _) => ident.fmt(f),
            External(Some(path)) => path.fmt(f),
            External(_) => f.write_str("."),
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

impl From<ast::AssignmentTarget> for Target {
    fn from(target: ast::AssignmentTarget) -> Self {
        use Target::*;

        match target {
            ast::AssignmentTarget::Noop => Noop,
            ast::AssignmentTarget::Internal(ident, path) => Internal(ident, path.map(Into::into)),
            ast::AssignmentTarget::External(path) => External(path.map(Into::into)),
        }
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub enum Variant<T, U> {
    Single { target: T, expr: Box<U> },
    Infallible { ok: T, err: T, expr: Box<U> },
}

impl<U> Expression for Variant<Target, U>
where
    U: Expression,
{
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Variant::*;

        let value = match self {
            Single { target, expr } => {
                let value = expr.resolve(ctx)?;
                target.insert(value.clone(), ctx);
                value
            }
            Infallible { ok, err, expr } => match expr.resolve(ctx) {
                Ok(value) => {
                    ok.insert(value.clone(), ctx);
                    err.insert(Value::Null, ctx);
                    value
                }
                Err(error) => {
                    ok.insert(Value::Null, ctx);
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
    #[error("useless no-op assignment")]
    UnneededNoop(Span),

    #[error("unhandled fallible assignment")]
    FallibleAssignment(String, String),

    #[error("unneeded error assignment")]
    InfallibleAssignment(String, String, Span, Span),
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
    fn labels(&self) -> Vec<Label> {
        use ErrorVariant::*;

        match &self.variant {
            UnneededNoop(target_span) => vec![
                Label::primary("this no-op assignment is useless", self.expr_span),
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
                Label::primary("this error assignment is unneeded", err_span),
                Label::context("because this expression cannot fail", self.expr_span),
                Label::context(format!("use: {} = {}", target, expr), ok_span),
            ],
        }
    }

    fn notes(&self) -> Vec<Note> {
        use ErrorVariant::*;

        match &self.variant {
            UnneededNoop(..) => vec![],
            FallibleAssignment(..) => vec![Note::SeeErrorDocs],
            InfallibleAssignment(..) => vec![Note::SeeErrorDocs],
        }
    }
}
