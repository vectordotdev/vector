use std::{convert::TryFrom, fmt};

use diagnostic::{DiagnosticMessage, Label, Note};
use lookup::lookup_v2::OwnedTargetPath;
use lookup::{LookupBuf, OwnedValuePath, PathPrefix, SegmentBuf};
use value::{Kind, Value};

use crate::{
    expression::{assignment::ErrorVariant::InvalidParentPathSegment, Expr, Resolved},
    parser::{
        ast::{self, Ident},
        Node,
    },
    state::{TypeInfo, TypeState},
    type_def::Details,
    value::kind::DefaultValue,
    CompileConfig, Context, Expression, Span, TypeDef,
};

#[derive(Clone, PartialEq)]
pub struct Assignment {
    variant: Variant<Target, Expr>,
}

impl Assignment {
    pub(crate) fn new(
        node: Node<Variant<Node<ast::AssignmentTarget>, Node<Expr>>>,
        state: &TypeState,
        fallible_rhs: Option<&dyn DiagnosticMessage>,
        config: &CompileConfig,
    ) -> Result<Self, Error> {
        let (_, variant) = node.take();

        let variant = match variant {
            Variant::Single { target, expr } => {
                let target_span = target.span();
                let expr_span = expr.span();
                let assignment_span = Span::new(target_span.start(), expr_span.start() - 1);

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
                verify_mutable(&target, config, expr_span, assignment_span)?;
                verify_overwritable(
                    &target,
                    state,
                    target_span,
                    expr_span,
                    assignment_span,
                    expr.clone(),
                )?;

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
                let type_def = expr.type_info(state).result;

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
                verify_mutable(&ok, config, expr_span, ok_span)?;
                verify_overwritable(
                    &ok,
                    state,
                    ok_span,
                    expr_span,
                    assignment_span,
                    expr.clone(),
                )?;

                let type_def = type_def.infallible();
                let default_value = type_def.default_value();

                // "err" target is assigned `null` or a string containing the
                // error message.
                let err = Target::try_from(err.into_inner())?;
                verify_mutable(&err, config, expr_span, err_span)?;
                verify_overwritable(
                    &err,
                    state,
                    err_span,
                    expr_span,
                    assignment_span,
                    expr.clone(),
                )?;

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
    config: &CompileConfig,
    expr_span: Span,
    assignment_span: Span,
) -> Result<(), Error> {
    match target {
        Target::External(target_path) => {
            if config.is_read_only_path(target_path) {
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

/// Ensure that the given target is allowed to be changed.
///
/// This returns an error if an assignment is done to an object field or array
/// index, while the parent of the field/index isn't an actual object/array.
fn verify_overwritable(
    target: &Target,
    state: &TypeState,
    target_span: Span,
    expr_span: Span,
    assignment_span: Span,
    rhs_expr: Expr,
) -> Result<(), Error> {
    let mut path = LookupBuf::from(target.path());

    let root_kind = match target {
        Target::Noop => Kind::any(),
        Target::Internal(ident, _) => state
            .local
            .variable(ident)
            .map_or_else(Kind::any, |detail| detail.type_def.kind().clone()),
        Target::External(_) => state.external.target_kind().clone(),
    };

    let mut parent_span = target_span;
    let mut remainder_str = String::new();

    // Walk the entire path from back to front. If the popped segment is a field
    // or index, check the segment before it, and ensure that its kind is an
    // object or array.
    while let Some(last) = path.pop_back() {
        let parent_kind = root_kind.at_path(&path);

        let (variant, segment_span, valid) = match last {
            segment @ (SegmentBuf::Field(_) | SegmentBuf::Coalesce(_)) => {
                let segment_str = segment.to_string();
                let segment_start = parent_span.end().saturating_sub(segment_str.len());
                let segment_span = Span::new(segment_start, parent_span.end());

                parent_span = Span::new(parent_span.start(), segment_start.saturating_sub(1));
                remainder_str.insert_str(0, &format!(".{segment_str}"));

                ("object", segment_span, parent_kind.contains_object())
            }
            SegmentBuf::Index(index) => {
                let segment_start = parent_span.end() - format!("[{index}]").len();
                let segment_span = Span::new(segment_start, parent_span.end());

                parent_span = Span::new(parent_span.start(), segment_start);
                remainder_str.insert_str(0, &format!("[{index}]"));

                ("array", segment_span, parent_kind.contains_array())
            }
        };

        if valid {
            continue;
        }

        let parent_str = match target {
            Target::Internal(ident, _) => format!("{ident}{path}"),
            Target::External(_) => {
                if path.is_root() && remainder_str.starts_with('.') {
                    remainder_str = remainder_str[1..].to_owned();
                }

                format!(".{path}")
            }
            Target::Noop => unreachable!(),
        };

        return Err(Error {
            variant: InvalidParentPathSegment {
                variant,
                parent_kind,
                parent_span,
                segment_span,
                parent_str,
                remainder_str,
                rhs_expr,
            },
            expr_span,
            assignment_span,
        });
    }

    Ok(())
}

impl Expression for Assignment {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.variant.resolve(ctx)
    }

    fn type_info(&self, state: &TypeState) -> TypeInfo {
        self.variant.type_info(state)
    }
}

impl fmt::Display for Assignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Variant::{Infallible, Single};

        match &self.variant {
            Single { target, expr } => write!(f, "{target} = {expr}"),
            Infallible { ok, err, expr, .. } => write!(f, "{ok}, {err} = {expr}"),
        }
    }
}

impl fmt::Debug for Assignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Variant::{Infallible, Single};

        match &self.variant {
            Single { target, expr } => write!(f, "{target:?} = {expr:?}"),
            Infallible { ok, err, expr, .. } => {
                write!(f, "Ok({ok:?}), Err({err:?}) = {expr:?}")
            }
        }
    }
}

// -----------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum Target {
    Noop,
    Internal(Ident, OwnedValuePath),
    External(OwnedTargetPath),
}

impl Target {
    fn insert_type_def(&self, state: &mut TypeState, new_type_def: TypeDef, value: Option<Value>) {
        match self {
            Self::Noop => {}
            Self::Internal(ident, path) => {
                let type_def = match state.local.variable(ident) {
                    None => TypeDef::null().with_type_inserted(&path.clone().into(), new_type_def),
                    Some(&Details { ref type_def, .. }) => type_def
                        .clone()
                        .with_type_inserted(&path.clone().into(), new_type_def),
                };

                let details = Details { type_def, value };
                state.local.insert_variable(ident.clone(), details);
            }

            Self::External(target_path) => {
                match target_path.prefix {
                    PathPrefix::Event => {
                        state.external.update_target(Details {
                            type_def: state
                                .external
                                .target()
                                .type_def
                                .clone()
                                .with_type_inserted(&target_path.path.clone().into(), new_type_def),
                            value,
                        });
                    }
                    PathPrefix::Metadata => {
                        let mut kind = state.external.metadata_kind().clone();
                        kind.insert(&target_path.path, new_type_def.kind().clone());
                        state.external.update_metadata(kind);
                    }
                };
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
                    Some(stored) => {
                        stored.insert(path, value);
                    }
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

    fn path(&self) -> OwnedValuePath {
        match self {
            Self::Noop => OwnedValuePath::root(),
            Self::Internal(_, path) => path.clone(),
            Self::External(target_path) => target_path.path.clone(),
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Target::{External, Internal, Noop};

        match self {
            Noop => f.write_str("_"),
            Internal(ident, path) if path.is_root() => ident.fmt(f),
            Internal(ident, path) => write!(f, "{ident}{path}"),
            External(path) => write!(f, "{path}"),
        }
    }
}

impl fmt::Debug for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Target::{External, Internal, Noop};

        match self {
            Noop => f.write_str("Noop"),
            Internal(ident, path) => {
                if path.is_root() {
                    write!(f, "Internal({ident})")
                } else {
                    write!(f, "Internal({ident}{path})")
                }
            }
            External(path) => write!(f, "External({path})"),
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
                    ast::QueryTarget::External(prefix) => {
                        External(OwnedTargetPath { prefix, path })
                    }
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
                Internal(ident, path.unwrap_or_else(OwnedValuePath::root))
            }
            ast::AssignmentTarget::External(path) => {
                External(path.unwrap_or_else(OwnedTargetPath::event_root))
            }
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

    fn type_info(&self, state: &TypeState) -> TypeInfo {
        let mut state = state.clone();
        match &self {
            Variant::Single { target, expr } => {
                let expr_result = expr.apply_type_info(&mut state);
                target.insert_type_def(&mut state, expr_result.clone(), expr.as_value());
                TypeInfo::new(state, expr_result)
            }
            Variant::Infallible {
                ok,
                err,
                expr,
                default,
            } => {
                let expr_result = expr.apply_type_info(&mut state);

                // The "ok" type is either the result of the expression, or a "default" value when the expression fails.
                let ok_type = expr_result
                    .clone()
                    .union(TypeDef::from(default.kind()))
                    .infallible();
                ok.insert_type_def(&mut state, ok_type, expr.as_value());

                // The "err" type is either the error message "bytes" or "null" (not undefined).
                let err_type = TypeDef::from(Kind::bytes().or_null());
                err.insert_type_def(&mut state, err_type, None);

                // Return type of the assignment expression itself is either the "expr" type or "bytes (the error message).
                let assignment_result = expr_result.infallible().or_bytes();

                TypeInfo::new(state, assignment_result)
            }
        }
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
            Single { target, expr } => write!(f, "{target} = {expr}"),
            Infallible { ok, err, expr, .. } => write!(f, "{ok}, {err} = {expr}"),
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
#[allow(clippy::large_enum_variant)]
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

    #[error("parent path segment rejects this mutation")]
    InvalidParentPathSegment {
        variant: &'static str,
        parent_kind: Kind,
        parent_span: Span,
        parent_str: String,
        segment_span: Span,
        remainder_str: String,
        rhs_expr: Expr,
    },
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
            InvalidParentPathSegment { .. } => 642,
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
                Label::context(format!("{target}, err = {expr}"), self.assignment_span),
            ],
            InfallibleAssignment(target, expr, ok_span, err_span) => vec![
                Label::primary("this error assignment is unnecessary", err_span),
                Label::context("because this expression can't fail", self.expr_span),
                Label::context(format!("use: {target} = {expr}"), ok_span),
            ],
            InvalidTarget(span) => vec![
                Label::primary("invalid assignment target", span),
                Label::context("use one of variable or path", span),
            ],
            ReadOnly => vec![Label::primary(
                "mutation of read-only value",
                self.assignment_span,
            )],
            InvalidParentPathSegment {
                variant,
                parent_kind,
                parent_span,
                segment_span,
                ..
            } => vec![
                Label::primary(
                    if variant == &"object" {
                        "querying a field of a non-object type is unsupported"
                    } else {
                        "indexing into a non-array type is unsupported"
                    },
                    segment_span,
                ),
                Label::context(
                    format!("this path resolves to a value of type {parent_kind}"),
                    parent_span,
                ),
            ],
        }
    }

    fn notes(&self) -> Vec<Note> {
        use ErrorVariant::{FallibleAssignment, InfallibleAssignment};

        match &self.variant {
            FallibleAssignment(..) | InfallibleAssignment(..) => vec![Note::SeeErrorDocs],
            InvalidParentPathSegment {
                variant,
                parent_str,
                remainder_str,
                rhs_expr,
                ..
            } => {
                let mut notes = vec![];

                notes.append(&mut Note::solution(
                    format!("change parent value to {variant}, before assignment"),
                    if variant == &"object" {
                        vec![
                            format!("{parent_str} = {{}}"),
                            format!("{parent_str}{remainder_str} = {rhs_expr}"),
                        ]
                    } else {
                        vec![
                            format!("{parent_str} = []"),
                            format!("{parent_str}{remainder_str} = {rhs_expr}"),
                        ]
                    },
                ));

                notes.push(Note::SeeErrorDocs);

                notes
            }
            _ => vec![],
        }
    }
}
