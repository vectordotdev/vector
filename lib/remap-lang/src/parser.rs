#![allow(clippy::or_fun_call)]

use crate::{
    diagnostic::{self, Diagnostic, DiagnosticList, Label, Note, Span},
    expression::{
        self, function, if_statement::IfCondition, Arithmetic, Array, Assignment, Block, Function,
        IfStatement, Literal, Map, Noop, Not, Path, Target, Variable,
    },
    path, state, Expr, Expression, Function as Fn, Operator, Value,
};
use pest::error::InputLocation;
use pest::iterators::{Pair, Pairs};
use regex::{Regex, RegexBuilder};
use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

pub(crate) type R = Rule;
type IResult<T> = Result<ParsedNode<T>, ParserBug>;
type PestError = pest::error::Error<R>;

#[derive(pest_derive::Parser)]
#[grammar = "../grammar.pest"]
pub(super) struct Parser<'a> {
    pub function_definitions: &'a [Box<dyn Fn>],
    pub allow_regex_return: bool,
    pub compiler_state: &'a mut state::Compiler,

    /// This field keeps track of the *recoverable* errors the parser
    /// encountered while parsing a program source.
    ///
    /// If the parser can continue after an error occurs, it is collected in
    /// this field. If a *fatal* error is encountered, the parses tries to move
    /// on to the next expression and records the failed expression as a
    /// `ParserBug`.
    ///
    /// The field is added to the `ParseError` return value once parsing is
    /// finished.
    ///
    /// All parsing functions take `self` such that this state cannot leak into
    /// subsequent parsing calls.
    diagnostics: DiagnosticList,
}

impl<'a> From<&Pair<'a, R>> for Span {
    fn from(pair: &Pair<R>) -> Self {
        pair.as_span().into()
    }
}

impl From<pest::Span<'_>> for Span {
    fn from(span: pest::Span) -> Self {
        (span.start()..span.end()).into()
    }
}

impl From<PestError> for Diagnostic {
    fn from(err: PestError) -> Self {
        let msg = "syntax error";

        let span = match err.location {
            InputLocation::Pos(start) => start..start,
            InputLocation::Span((start, end)) => start..end,
        };

        let label = Label::primary("invalid token", span.clone());

        match err.variant {
            pest::error::ErrorVariant::ParsingError {
                positives,
                negatives,
            } => {
                let expected = if positives.len() == 1 {
                    Label::context(format!("expected: {}", positives[0]), span.clone())
                } else {
                    Label::context(
                        format!(
                            "expected one of: {}",
                            positives
                                .iter()
                                .map(|r| r.to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        span.clone(),
                    )
                };

                let unexpected = Label::primary(
                    format!(
                        "unexpected: {}",
                        negatives
                            .iter()
                            .map(|r| r.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    span.clone(),
                );

                match () {
                    _ if !positives.is_empty() => Diagnostic::error(msg)
                        .with_label(label)
                        .with_label(expected),
                    _ if !negatives.is_empty() => Diagnostic::error(msg)
                        .with_label(label)
                        .with_label(unexpected),
                    _ => Diagnostic::error(msg).with_primary("unexpected token", span),
                }
            }
            pest::error::ErrorVariant::CustomError { message } => {
                Diagnostic::error(msg).with_primary(message, span)
            }
        }
    }
}

// -----------------------------------------------------------------------------

/// A non-recoverable error raised by the parser.
///
/// The parser tries to recover the next expression it tries to parse, if
/// possible.
#[derive(Debug, Clone)]
pub(crate) struct ParserBug(Span, R);

impl From<ParserBug> for Diagnostic {
    fn from(err: ParserBug) -> Self {
        Diagnostic::bug("unexpected token").with_primary(err.1.to_string(), err.0)
    }
}

// -----------------------------------------------------------------------------

/// A container type that wraps an [`Expression`] and adds a span pointing to
/// the expression position within the parsed source.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedExpression {
    span: Span,
    expr: Expr,
}

impl ParsedExpression {
    pub fn span(&self) -> Span {
        self.span
    }
}

impl Deref for ParsedExpression {
    type Target = Expr;

    fn deref(&self) -> &Self::Target {
        &self.expr
    }
}

impl DerefMut for ParsedExpression {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.expr
    }
}

impl<T: Into<Expr>> From<ParsedNode<T>> for ParsedExpression {
    fn from(node: ParsedNode<T>) -> Self {
        Self {
            span: node.span,
            expr: node.inner.into(),
        }
    }
}

// -----------------------------------------------------------------------------

/// Similar to [`ParsedExpression`] except that it is private, generic over `T`
/// and has an expanded API used within the parser.
#[derive(Clone, Debug, PartialEq)]
struct ParsedNode<T> {
    span: Span,
    inner: T,
}

impl<T> Deref for ParsedNode<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for ParsedNode<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> ParsedNode<T> {
    fn into_inner(self) -> T {
        self.inner
    }

    fn take(self) -> (Span, T) {
        (self.span, self.inner)
    }

    #[allow(clippy::wrong_self_convention)]
    fn to_expr(self) -> ParsedNode<Expr>
    where
        T: Into<Expr>,
    {
        let (span, inner) = self.take();

        ParsedNode {
            span,
            inner: inner.into(),
        }
    }
}

impl<T, U: Into<T>, S: Into<Span>> From<(S, U)> for ParsedNode<T> {
    fn from((span, node): (S, U)) -> Self {
        Self {
            span: span.into(),
            inner: node.into(),
        }
    }
}

// -----------------------------------------------------------------------------

// Auto-generate a set of parser functions to parse different operations.
macro_rules! operation_fns {
    (@impl $($rule:tt => { op: [$head_op:path, $($tail_op:path),+ $(,)?], next: $next:tt, })+) => (
        $(
            paste::paste! {
                fn [<$rule _from_pair>](&mut self, pair: Pair<R>) -> IResult<Expr> {
                    let span = Span::from(&pair);
                    let mut pairs = pair.into_inner();

                    let next = pairs.next().ok_or(e(R::$rule, span))?;
                    let (span, mut lhs) = self.[<$next _from_pair>](next)?.take();
                    let mut op = Operator::$head_op;

                    for pair in pairs {
                        match pair.as_rule() {
                            R::[<operator_ $rule>] => {
                                op = Operator::from_str(pair.as_str()).map_err(|_| e(R::$rule, span))?;
                            }
                            _ => {
                                lhs = Expr::from(Arithmetic::new(
                                    Box::new(lhs),
                                    Box::new(self.[<$next _from_pair>](pair)?.into_inner()),
                                    op.clone(),
                                ));
                            }
                        }
                    }

                    Ok((span, lhs).into())
                }
            }
        )+
    );

    ($($rule:tt => { op: [$($op:path),+ $(,)?], next: $next:tt, })+) => (
        operation_fns!(@impl $($rule => { op: [$($op),+], next: $next, })+);
    );
}

impl<'a> Parser<'a> {
    pub fn new(
        function_definitions: &'a [Box<dyn Fn>],
        compiler_state: &'a mut state::Compiler,
        allow_regex_return: bool,
    ) -> Self {
        Self {
            function_definitions,
            allow_regex_return,
            compiler_state,
            diagnostics: DiagnosticList::default(),
        }
    }

    /// Parse a source string into a valid [`Program`].
    pub(crate) fn program_from_str(
        mut self,
        source: &'a str,
    ) -> diagnostic::Result<Vec<ParsedExpression>> {
        let expressions = self
            .pairs_from_str(R::program, source)
            .and_then(|pairs| self.pairs_to_expressions(pairs.into_inner()))
            .map(|node| node.into_inner())
            .map(|nodes| nodes.into_iter().map(Into::into).collect::<Vec<_>>())
            .map_err(|err| self.diagnostics.push(err.into()));

        match expressions {
            Err(_) => Err(self.diagnostics),
            Ok(_) if self.diagnostics.is_err() => Err(self.diagnostics),
            Ok(expressions) => Ok((expressions, self.diagnostics)),
        }
    }

    /// Parse a string path into a [`path::Path`] wrapper with easy access to
    /// individual path [`path::Segment`]s.
    pub(crate) fn path_from_str(mut self, path: &'a str) -> diagnostic::Result<path::Path> {
        let path = self
            .pairs_from_str(R::rule_path, path)
            .and_then(|pairs| pairs.into_inner().next().ok_or(e(R::rule_path, path)))
            .and_then(|pair| self.path_from_pair(pair))
            .map(|node| node.into_inner())
            .map_err(|err| self.diagnostics.push(err.into()));

        match path {
            Err(_) => Err(self.diagnostics),
            Ok(_) if self.diagnostics.is_err() => Err(self.diagnostics),
            Ok(path) => Ok((path, self.diagnostics)),
        }
    }

    /// Parse a string into a [`path::Field`] wrapper.
    ///
    /// Depending on the provided string, this can result in three outcomes:
    ///
    /// - A `Field::Regular` if the string is a valid "identifier".
    /// - A `Field::Quoted` if the string is a valid "quoted string".
    /// - An error if neither is true.
    ///
    /// These rules are defined by the Remap parser.
    pub(crate) fn path_field_from_str(mut self, field: &'a str) -> diagnostic::Result<path::Field> {
        let field = self
            .pairs_from_str(R::rule_ident, field)
            .and_then(|pairs| pairs.into_inner().next().ok_or(e(R::rule_ident, field)))
            .map(|pair| path::Field::Regular(pair.as_str().to_owned()))
            .or_else(|_| {
                self.diagnostics.clear();

                self.pairs_from_str(R::rule_string_inner, field)
                    .map(|node| node.into_inner())
                    .and_then(|mut pairs| pairs.next().ok_or(e(R::rule_string_inner, field)))
                    .map(|pair| path::Field::Quoted(pair.as_str().to_owned()))
            })
            .map_err(|err| self.diagnostics.push(err.into()));

        match field {
            Err(_) => Err(self.diagnostics),
            Ok(_) if self.diagnostics.is_err() => Err(self.diagnostics),
            Ok(field) => Ok((field, self.diagnostics)),
        }
    }

    /// Converts the set of known "root" rules into boxed [`Expression`] trait
    /// objects.
    fn pairs_to_expressions(&mut self, pairs: Pairs<'a, R>) -> IResult<Vec<ParsedNode<Expr>>> {
        let mut nodes = vec![];

        for pair in pairs {
            match pair.as_rule() {
                R::assignment | R::boolean_expr | R::block | R::if_statement => {
                    nodes.push(self.expression_from_pair(pair)?)
                }
                R::EOI => (),
                _ => return Err(e(R::expression, &pair)),
            }
        }

        if let Some(node) = nodes.last() {
            let td = node.type_def(&self.compiler_state);

            if !self.allow_regex_return && !td.kind.is_all() && td.scalar_kind().contains_regex() {
                self.diagnostics.push(
                    Diagnostic::error("invalid return value")
                        .with_primary("regex return value not allowed", node.span),
                );
            }
        }

        let start = nodes.first().map(|n| n.span.start).unwrap_or_default();
        let end = nodes.last().map(|n| n.span.end).unwrap_or_default();

        Ok((start..end, nodes).into())
    }

    fn pairs_from_str<'b>(&mut self, rule: R, source: &'b str) -> IResult<Pairs<'b, R>> {
        use pest::Parser;

        let span = Span::from(source);
        let pairs = match Self::parse(rule, source) {
            Ok(pairs) => pairs,
            Err(err) => {
                self.diagnostics.push(err.into());
                pest::state("", Ok).unwrap()
            }
        };

        Ok((span, pairs).into())
    }

    /// Given a `Pair`, build a boxed [`Expression`] trait object from it.
    fn expression_from_pair(&mut self, pair: Pair<R>) -> IResult<Expr> {
        match pair.as_rule() {
            R::assignment => self.assignment_from_pair(pair),
            R::boolean_expr => self.boolean_expr_from_pair(pair),
            R::block => self.block_from_pair(pair),
            R::if_statement => self.if_statement_from_pair(pair),
            _ => Err(e(R::expression, &pair)),
        }
    }

    fn assignment_from_pair(&mut self, pair: Pair<R>) -> IResult<Expr> {
        let span = Span::from(&pair);
        let mut pairs = pair.into_inner();

        let (target_span, mut target) = self
            .target_from_pair(pairs.next().ok_or(e(R::assignment, span))?)?
            .take();
        let (expression_span, expression) = self
            .expression_from_pair(pairs.next().ok_or(e(R::assignment, span))?)?
            .take();

        let assignment_span = target_span.start..expression_span.end;

        if let Target::Infallible { ok, err } = &target {
            if !expression.type_def(&self.compiler_state).is_fallible() {
                self.diagnostics.push(
                    Diagnostic::error("unneeded error assignment")
                        .with_primary("this error assignment is unneeded", target_span)
                        .with_context("because this expression cannot fail", expression_span)
                        .with_note(Note::InfallibleAssignment {
                            ok: ok.to_string(),
                            err: err.to_string(),
                        }),
                );

                target = Target::Variable(Variable::new("_".to_owned(), None));
            }
        }

        let assignment = Assignment::new(target, Box::new(expression), &mut self.compiler_state);

        Ok((assignment_span, assignment).into())
    }

    /// Return the target type to which a value is being assigned.
    ///
    /// This can either return a `variable` or a `target_path` target, depending
    /// on the parser rule being processed.
    fn target_from_pair(&mut self, pair: Pair<R>) -> IResult<Target> {
        match pair.as_rule() {
            R::variable => self.variable_from_pair(pair).map(|node| {
                let (span, mut variable) = node.take();

                // track an error diagnostic and re-assign variable to a
                // variable without a path, since we don't support this
                // currently.
                if let Some(path) = variable.path() {
                    let path_span = span.end - path.to_string().bytes().len()..span.end;
                    let variable_span = span.start..path_span.start;

                    self.diagnostics.push(
                        Diagnostic::error("path-based variable assignment")
                            .with_primary(
                                "assignment to variable path currently unsupported",
                                path_span,
                            )
                            .with_context(
                                format!(r#"assign to "{}" instead"#, variable.ident()),
                                variable_span,
                            ),
                    );

                    variable = Variable::new("_".to_owned(), None);
                }

                (span, Target::Variable(variable)).into()
            }),
            R::path => {
                let (span, path) = self.path_from_pair(pair)?.take();
                Ok((span, Target::Path(Path::new(path))).into())
            }
            R::target_infallible => self
                .target_infallible_from_pair(pair)
                .map(|node| node.take().into()),
            _ => Err(e(R::target, &pair)),
        }
    }

    fn target_infallible_from_pair(&mut self, pair: Pair<R>) -> IResult<Target> {
        let span = Span::from(&pair);
        let mut pairs = pair.into_inner();

        let (ok_span, ok) = pairs
            .next()
            .ok_or(e(R::target_infallible, span))
            .and_then(|pair| Ok(self.target_from_pair(pair)?))?
            .take();

        let (err_span, err) = pairs
            .next()
            .ok_or(e(R::target_infallible, span))
            .and_then(|pair| Ok(self.target_from_pair(pair)?))?
            .take();

        let (ok, err) = (Box::new(ok), Box::new(err));

        Ok((
            Span::new(ok_span.start, err_span.end),
            Target::Infallible { ok, err },
        )
            .into())
    }

    /// Parse block expressions.
    fn block_from_pair(&mut self, pair: Pair<R>) -> IResult<Expr> {
        let span = Span::from(&pair);
        let mut expressions = vec![];

        for pair in pair.into_inner() {
            expressions.push(self.expression_from_pair(pair)?.into_inner());
        }

        Ok((span, Block::new(expressions)).into())
    }

    /// Parse if-statement expressions.
    fn if_statement_from_pair(&mut self, pair: Pair<R>) -> IResult<Expr> {
        self.compiler_state.track_changes();

        let span = Span::from(&pair);
        let mut pairs = pair.into_inner();

        // if condition
        let conditional = self
            .if_condition_from_pair(pairs.next().ok_or(e(R::if_statement, span))?)?
            .into_inner();

        // If the conditional failed to parse, it has created an error
        // diagnostic. We return a no-op expression. This allows us to continue
        // parsing.
        let conditional = match self.try_or_noop(conditional, span) {
            Ok(conditional) => conditional,
            Err(noop) => return noop,
        };

        let true_expression = self
            .expression_from_pair(pairs.next().ok_or(e(R::if_statement, span))?)?
            .into_inner();

        // else condition
        let mut false_expression = pairs
            .next_back()
            .map(|pair| self.expression_from_pair(pair))
            .transpose()?
            .map(ParsedNode::into_inner)
            .unwrap_or_else(|| Expr::from(Noop));

        let mut pairs = pairs.rev().peekable();

        // optional if-else conditions
        while let Some(pair) = pairs.next() {
            let (conditional, true_expression) = match pairs.peek().map(Pair::as_rule) {
                Some(R::block) | None => {
                    let conditional = self.if_condition_from_pair(pair)?.into_inner();
                    let conditional = match self.try_or_noop(conditional, span) {
                        Ok(conditional) => conditional,
                        Err(noop) => return noop,
                    };

                    let true_expression = false_expression;
                    false_expression = Noop.into();

                    (conditional, true_expression)
                }
                Some(R::if_condition) => {
                    let next_pair = pairs.next().ok_or(e(R::if_statement, span))?;

                    let conditional = self.if_condition_from_pair(next_pair)?.into_inner();
                    let conditional = match self.try_or_noop(conditional, span) {
                        Ok(conditional) => conditional,
                        Err(noop) => return noop,
                    };

                    let true_expression = self.expression_from_pair(pair)?.into_inner();

                    (conditional, true_expression)
                }
                _ => return Err(e(R::if_statement, span)),
            };

            false_expression = IfStatement::new(
                conditional,
                Box::new(true_expression),
                Box::new(false_expression),
            )
            .into();
        }

        let node = IfStatement::new(
            conditional,
            Box::new(true_expression),
            Box::new(false_expression),
        );

        Ok((span, node).into())
    }

    fn if_condition_from_pair(&mut self, pair: Pair<R>) -> IResult<Result<IfCondition, ()>> {
        let span = Span::from(&pair);
        let mut pairs = pair.clone().into_inner();

        let (span, expression) = if let Some(R::boolean_expr) = pairs.peek().map(|p| p.as_rule()) {
            let pair = pairs.next().ok_or(e(R::if_condition, span))?;
            self.expression_from_pair(pair)?.take()
        } else {
            self.block_from_pair(pair)?.take()
        };

        // If the condition is invalid, we add a diagnostic error. We also let
        // the callee know parsing the condition failed.
        let result = IfCondition::new(Box::new(expression), &self.compiler_state)
            .map_err(|err| self.diagnostics.push((span, err).into()));

        Ok((span, result).into())
    }

    /// Parse not operator, or fall-through to primary values or function calls.
    fn not_from_pair(&mut self, pair: Pair<R>) -> IResult<Expr> {
        let span = Span::from(&pair);
        let pairs = pair.into_inner();

        let mut count = 0;
        let mut expression = Expr::from(Noop);

        for pair in pairs {
            match pair.as_rule() {
                R::operator_not => count += 1,
                R::primary => expression = self.primary_from_pair(pair)?.into_inner(),
                R::call => expression = self.call_from_pair(pair)?.into_inner(),
                _ => return Err(e(R::not, &pair)),
            }
        }

        if count % 2 != 0 {
            expression = Expr::from(Not::new(Box::new(expression)))
        }

        Ok((span, expression).into())
    }

    /// Parse one of possible primary expressions.
    fn primary_from_pair(&mut self, pair: Pair<R>) -> IResult<Expr> {
        let span = Span::from(&pair);
        let pair = pair.into_inner().next().ok_or(e(R::primary, span))?;

        match pair.as_rule() {
            R::value => self.literal_from_pair(pair.into_inner().next().ok_or(e(R::value, span))?),
            R::variable => self.variable_from_pair(pair).map(ParsedNode::to_expr),
            R::path => self.path_from_pair(pair).map(|node| {
                let (span, path) = node.take();
                (span, Path::new(path)).into()
            }),
            R::group => {
                self.expression_from_pair(pair.into_inner().next().ok_or(e(R::group, span))?)
            }
            _ => Err(e(R::primary, &pair)),
        }
    }

    /// Parse a [`Value`] into a [`Literal`] expression.
    fn literal_from_pair(&mut self, pair: Pair<R>) -> IResult<Expr> {
        let span = Span::from(&pair);

        match pair.as_rule() {
            R::string => self.string_from_pair(pair).map(ParsedNode::to_expr),
            R::null => Ok((span, Literal::from(Value::Null)).into()),
            R::boolean => Ok((span, Literal::from(pair.as_str() == "true")).into()),
            R::integer => Ok((
                span,
                Literal::from(
                    pair.as_str()
                        .parse::<i64>()
                        .map_err(|_| e(R::integer, &pair))?,
                ),
            )
                .into()),
            R::float => Ok((
                span,
                Literal::from(
                    pair.as_str()
                        .parse::<f64>()
                        .map_err(|_| e(R::float, &pair))?,
                ),
            )
                .into()),
            R::array => self.array_from_pair(pair).map(ParsedNode::to_expr),
            R::map => self.map_from_pair(pair).map(ParsedNode::to_expr),
            R::regex => self.regex_from_pair(pair).map(ParsedNode::to_expr),
            _ => Err(e(R::value, &pair)),
        }
    }

    fn array_from_pair(&mut self, pair: Pair<R>) -> IResult<Array> {
        let span = Span::from(&pair);

        let expressions = pair
            .into_inner()
            .map(|pair| self.expression_from_pair(pair).map(ParsedNode::into_inner))
            .collect::<Result<Vec<_>, _>>()?;

        Ok((span, Array::new(expressions)).into())
    }

    fn map_from_pair(&mut self, pair: Pair<R>) -> IResult<Map> {
        let span = Span::from(&pair);

        let map = pair
            .into_inner()
            .map(|pair| self.kv_from_pair(pair).map(ParsedNode::into_inner))
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        Ok((span, Map::new(map)).into())
    }

    fn kv_from_pair(&mut self, pair: Pair<R>) -> IResult<(String, Expr)> {
        let span = Span::from(&pair);
        let mut inner = pair.into_inner();

        let pair = inner.next().ok_or(e(R::kv_pair, span))?;
        let (key_span, key) = self.string_from_pair(pair)?.take();

        let pair = inner.next().ok_or(e(R::kv_pair, span))?;
        let (expr_span, expr) = self.expression_from_pair(pair)?.take();

        Ok((key_span.start..expr_span.end, (key, expr)).into())
    }

    /// Parse function call expressions.
    fn call_from_pair(&mut self, pair: Pair<R>) -> IResult<Expr> {
        self.compiler_state.track_changes();

        let span = Span::from(&pair);
        let mut inner = pair.into_inner();

        let ident = inner.next().ok_or(e(R::call, span))?.as_str();
        let ident_span = span.start..=ident.bytes().len();

        let abort_on_error = match inner.peek().map(|p| p.as_rule()) {
            Some(R::bang) => {
                inner.next();
                true
            }
            _ => false,
        };

        let (arguments_span, arguments) = inner
            .next()
            .map(|pair| self.arguments_from_pair(pair))
            .transpose()?
            .map(|s| s.take())
            .unwrap_or_else(|| (Span::default(), vec![]));

        let function = Function::new(
            ident,
            abort_on_error,
            arguments,
            &self.function_definitions,
            &self.compiler_state,
        );

        let expression: Expr = match function {
            Ok(function) => function.into(),
            Err(err) => {
                self.compiler_state.revert_changes();

                self.diagnostics.push(match err {
                    function::Error::Undefined => Diagnostic::error("call to undefined function")
                        .with_primary("undefined function", ident_span),
                    function::Error::ArityMismatch { max, got } => {
                        Diagnostic::error("function argument arity mismatch")
                            .with_primary(format!("got: {}", got), arguments_span)
                            .with_context(format!("expected: {} (at most)", max), arguments_span)
                    }
                    // TODO: have spans for each individual keyword
                    function::Error::UnknownKeyword(kw) => {
                        Diagnostic::error("unknown function argument keyword")
                            .with_primary(format!("unknown keyword: {}", kw), arguments_span)
                    }
                    function::Error::AbortInfallible => {
                        let bang_span = *ident_span.end() + 1..*ident_span.end() + 1;

                        Diagnostic::error("cannot abort function that never fails")
                            .with_primary("this function cannot fail", ident_span)
                            .with_context("remove this abort-instruction", bang_span)
                            .with_note(Note::SeeErrDocs)
                    }
                    function::Error::MissingArg { argument, .. } => {
                        Diagnostic::error("function argument missing")
                            .with_primary(format!("required argument missing: {}", argument), span)
                    }
                    function::Error::Compile(message) => {
                        Diagnostic::error("unable to parse function").with_primary(message, span)
                    }
                    function::Error::Argument(arg, err) => {
                        Diagnostic::error("function argument error")
                            .with_primary(format!("{}: {}", arg, err), arguments_span)
                    }
                });

                Noop.into()
            }
        };

        Ok((span, expression).into())
    }

    /// Parse into a vector of argument properties.
    fn arguments_from_pair(&mut self, pair: Pair<R>) -> IResult<Vec<(Option<String>, Expr)>> {
        let span = Span::from(&pair);

        let arguments = pair
            .into_inner()
            .map(|pair| self.argument_from_pair(pair).map(ParsedNode::into_inner))
            .collect::<Result<Vec<_>, _>>()?;

        Ok((span, arguments).into())
    }

    /// Parse optional argument keyword and [`Argument`] value.
    fn argument_from_pair(&mut self, pair: Pair<R>) -> IResult<(Option<String>, Expr)> {
        let span = Span::from(&pair);
        let mut ident = None;

        for pair in pair.into_inner() {
            match pair.as_rule() {
                // This matches first, if a keyword is provided.
                R::ident => ident = Some(pair.as_str().to_owned()),
                _ => {
                    return Ok(
                        (span, (ident, self.expression_from_pair(pair)?.into_inner())).into(),
                    )
                }
            }
        }

        Err(e(R::argument, span))
    }

    /// Parse a [`Regex`] value
    fn regex_from_pair(&mut self, pair: Pair<R>) -> IResult<Regex> {
        let span = Span::from(&pair);
        let mut inner = pair.into_inner();

        let pattern = inner
            .next()
            .ok_or(e(R::regex_inner, span))?
            .as_str()
            .replace("\\/", "/");

        let (x, i, m) = inner
            .next()
            .map(|flags| {
                flags
                    .as_str()
                    .chars()
                    .fold((false, false, false), |(x, i, m), flag| match flag {
                        'x' => (true, i, m),
                        'i' => (x, true, m),
                        'm' => (x, i, true),
                        _ => (x, i, m),
                    })
            })
            .unwrap_or_default();

        #[allow(clippy::trivial_regex)]
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(i)
            .multi_line(m)
            .ignore_whitespace(x)
            .build()
            .map_err(|err| {
                let error = err
                    .to_string()
                    .split("error: ")
                    .last()
                    .unwrap_or("unknown error")
                    .to_owned();

                // Record error diagnostic for invalid regex.
                self.diagnostics.push(
                    Diagnostic::error("regex parsing unsuccessful")
                        .with_primary("invalid regex", span)
                        .with_primary(format!("error: {}", error), span),
                )
            })
            .unwrap_or_else(|_| Regex::new("").unwrap());

        Ok((span, regex).into())
    }

    /// Parse a [`Path`] value, e.g. ".foo.bar"
    fn path_from_pair(&self, pair: Pair<R>) -> IResult<path::Path> {
        let span = Span::from(&pair);
        let mut pairs = pair.into_inner();

        // If no segments are provided, it's the root path (e.g. `.`).
        let path_segments = match pairs.next() {
            Some(path_segments) => path_segments,
            None => return Ok((span, path::Path::root()).into()),
        };

        let segments = match path_segments.as_rule() {
            R::path_segments => self.path_segments_from_pair(path_segments)?,
            _ => return Err(e(R::path, &path_segments)),
        };

        Ok((span, path::Path::new_unchecked(segments.into_inner())).into())
    }

    fn path_segments_from_pair(&self, pair: Pair<R>) -> IResult<Vec<path::Segment>> {
        let span = Span::from(&pair);

        let segments: Vec<path::Segment> = pair
            .into_inner()
            .map(|pair| match pair.as_rule() {
                R::path_index => self.path_index_from_pair(pair).map(ParsedNode::into_inner),
                R::path_segment => self
                    .path_segment_from_pair(pair)
                    .map(ParsedNode::into_inner),
                _ => Err(e(R::path_segments, &pair)),
            })
            .collect::<Result<_, _>>()?;

        Ok((span, segments).into())
    }

    fn path_segment_from_pair(&self, pair: Pair<R>) -> IResult<path::Segment> {
        let span = Span::from(&pair);
        let segment = pair.into_inner().next().ok_or(e(R::path_segment, span))?;

        match segment.as_rule() {
            R::path_field => self.path_field_segment_from_pair(segment),
            R::path_coalesce => self.path_coalesce_segment_from_pair(segment),
            _ => Err(e(R::path_segment, &segment)),
        }
    }

    fn path_field_segment_from_pair(&self, pair: Pair<R>) -> IResult<path::Segment> {
        self.path_field_from_pair(pair).map(|node| {
            let (span, field) = node.take();
            (span, path::Segment::Field(field)).into()
        })
    }

    fn path_coalesce_segment_from_pair(&self, pair: Pair<R>) -> IResult<path::Segment> {
        let span = Span::from(&pair);

        let fields = pair
            .into_inner()
            .map(|pair| self.path_field_from_pair(pair).map(ParsedNode::into_inner))
            .collect::<Result<Vec<_>, _>>()?;

        Ok((span, path::Segment::Coalesce(fields)).into())
    }

    fn path_field_from_pair(&self, pair: Pair<R>) -> IResult<path::Field> {
        let span = Span::from(&pair);
        let field = pair.into_inner().next().ok_or(e(Rule::path_field, span))?;

        match field.as_rule() {
            R::string => Ok((
                span,
                path::Field::Quoted(self.string_from_pair(field)?.into_inner()),
            )
                .into()),
            R::field => Ok((span, path::Field::Regular(field.as_str().to_owned())).into()),
            _ => Err(e(R::path_field, &field)),
        }
    }

    fn path_index_from_pair(&self, pair: Pair<R>) -> IResult<path::Segment> {
        let span = Span::from(&pair);
        let index = pair
            .into_inner()
            .next()
            .ok_or(e(R::path_index, span))?
            .as_str()
            .parse::<usize>()
            .map_err(|_| e(R::path_index_inner, span))?;

        Ok((span, path::Segment::Index(index)).into())
    }

    /// Parse a [`Variable`] value, e.g. "$foo"
    fn variable_from_pair(&self, pair: Pair<R>) -> IResult<Variable> {
        let span = Span::from(&pair);
        let mut inner = pair.into_inner();

        let ident = inner
            .next()
            .ok_or(e(R::variable, span))?
            .as_str()
            .to_owned();

        let segments = inner.try_fold(vec![], |mut segments, pair| {
            match pair.as_rule() {
                R::path_index => segments.push(self.path_index_from_pair(pair)?.into_inner()),
                R::path_segments => {
                    segments.append(&mut self.path_segments_from_pair(pair)?.into_inner())
                }
                _ => return Err(e(R::variable, &pair)),
            };

            Ok(segments)
        })?;

        let expr = match segments {
            _ if segments.is_empty() => None,
            _ => {
                let path = path::Path::new_unchecked(segments);
                Some(expression::Path::new(path))
            }
        };

        Ok((span, Variable::new(ident, expr)).into())
    }

    fn string_from_pair(&self, pair: Pair<R>) -> IResult<String> {
        let span = Span::from(&pair);
        let string = pair.into_inner().next().ok_or(e(R::string, span))?;
        self.escaped_string_from_pair(string)
    }

    fn escaped_string_from_pair(&self, pair: Pair<R>) -> IResult<String> {
        let span = Span::from(&pair);

        // This is only executed once per string at parse time, and so I'm not
        // losing sleep over the reallocation. However, if we want to mutate the
        // underlying string then we can take some inspiration from:
        //
        // https://github.com/rust-lang/rust/blob/master/src/librustc_lexer/src/unescape.rs

        let literal_str = pair.as_str();
        let mut escaped_chars: Vec<char> = Vec::with_capacity(literal_str.len());

        let mut is_escaped = false;
        for c in literal_str.chars() {
            if is_escaped {
                match c {
                    '\\' => escaped_chars.push(c),
                    'n' => escaped_chars.push('\n'),
                    't' => escaped_chars.push('\t'),
                    '"' => escaped_chars.push('"'),
                    _ => return Err(e(Rule::char, &pair)),
                }
                is_escaped = false;
            } else if c == '\\' {
                is_escaped = true;
            } else {
                escaped_chars.push(c);
            }
        }

        Ok((span, escaped_chars.into_iter().collect::<String>()).into())
    }

    /// Allows you to check if a result passed. If it did, the compiler starts
    /// tracking new changes, if it didn't, the existing state is overwritten
    /// with the previously recorded changes (if any).
    fn try_or_noop<T, E>(&mut self, value: Result<T, E>, span: Span) -> Result<T, IResult<Expr>> {
        match value {
            Ok(value) => {
                self.compiler_state.track_changes();
                Ok(value)
            }
            Err(_) => {
                self.compiler_state.revert_changes();
                Err(Ok((span, Noop).into()))
            }
        }
    }

    // The operations are defined in reverse order, meaning boolean expressions are
    // computed first, and multiplication last.
    //
    // The order of `op` operations defines operator precedence.
    operation_fns! {
        multiplication => {
            op: [Multiply, Divide, IntegerDivide, Remainder],
            next: not,
        }

        addition => {
            op: [Add, Subtract],
            next: multiplication,
        }

        comparison => {
            op: [Greater, GreaterOrEqual, Less, LessOrEqual],
            next: addition,
        }

        equality => {
            op: [Equal, NotEqual],
            next: comparison,
        }

        boolean_expr => {
            op: [ErrorOr, And, Or],
            next: equality,
        }
    }
}

// -----------------------------------------------------------------------------

#[inline]
fn e(rule: R, span: impl Into<Span>) -> ParserBug {
    ParserBug(span.into(), rule)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_root_path() {
        let cases = vec![
            (
                ".",
                vec![],
                Ok(vec![Path::new(path::Path::new_unchecked(vec![])).into()]),
            ),
            (
                " . ",
                vec![],
                Ok(vec![Path::new(path::Path::new_unchecked(vec![])).into()]),
            ),
            (
                ".\n",
                vec![],
                Ok(vec![Path::new(path::Path::new_unchecked(vec![])).into()]),
            ),
            (
                "\n.",
                vec![],
                Ok(vec![Path::new(path::Path::new_unchecked(vec![])).into()]),
            ),
            (
                "\n.\n",
                vec![],
                Ok(vec![Path::new(path::Path::new_unchecked(vec![])).into()]),
            ),
            // TODO: move to `remap-tests`
            // ("..", vec![" 1:2\n", "= expected path segment"], Ok(vec![])),
            // (". bar", vec![" 1:3\n", "= expected operator"], Ok(vec![])),
            // (
            //     r#". "bar""#,
            //     vec![" 1:2\n", "= expected path segment"], // TODO: improve error message
            //     Ok(vec![]),
            // ),
        ];

        validate_rule(cases);
    }

    #[allow(clippy::type_complexity)]
    fn validate_rule(cases: Vec<(&str, Vec<&str>, Result<Vec<Expr>, ()>)>) {
        for (mut i, (source, compile_check, run_check)) in cases.into_iter().enumerate() {
            let compile_check: Vec<&str> = compile_check;
            i += 1;

            let mut state = state::Compiler::default();
            let parser = Parser::new(&[], &mut state, true);
            let pairs = parser
                .program_from_str(source)
                .map_err(|err| diagnostic::Formatter::new(source, err).to_string());

            match pairs {
                Ok((got, _)) => {
                    if compile_check.is_empty() {
                        let got = got.into_iter().map(|e| e.expr).collect();

                        assert_eq!(Ok(got), run_check, "test case: {}", i)
                    } else {
                        for exp in compile_check {
                            assert!(
                                "".contains(exp),
                                "expected error: {}\nwith source: {}\nresult: {:?}\n test case {}",
                                exp,
                                source,
                                got,
                                i
                            );
                        }
                    }
                }
                Err(err) if !compile_check.is_empty() => {
                    for exp in compile_check {
                        assert!(
                            err.contains(exp),
                            "expected: {}\nwith source: {}\nfull error message: {}\n test case {}",
                            exp,
                            source,
                            err,
                            i
                        );
                    }
                }
                Err(err) => panic!("expected no error, got \"{}\" for test case {}", err, i),
            }
        }
    }

    // TODO: move to `remap-tests`
    // #[test]
    // fn check_parser_errors() {
    //     let cases = vec![
    //         (
    //             ".foo bar",
    //             vec![
    //                 " 1:6\n",
    //                 "= expected operator",
    //             ],
    //         ),
    //         (
    //             ".=",
    //             vec![
    //                 " 1:3\n",
    //                 "= expected assignment, if-statement, query, or block",
    //             ],
    //         ),
    //         (
    //             ".foo = !",
    //             vec![
    //                 " 1:9\n",
    //                 "= expected value, variable, path, group or function call, value, variable, path, group, !",
    //             ],
    //         ),
    //         (
    //             r#".foo.bar = "baz" and this"#,
    //             vec![
    //                 " 1:18\n",
    //                 "= expected operator",
    //             ],
    //         ),
    //         (r#".foo.bar = "baz" +"#, vec![" 1:19", "= expected query"]),
    //         (
    //             ".foo.bar = .foo.(bar |)",
    //             vec![" 1:23\n", "= expected path field"],
    //         ),
    //         (
    //             r#"if .foo > 0 { .foo = "bar" } else"#,
    //             vec![" 1:34\n", "= expected block"],
    //         ),
    //         (
    //             "if .foo { }",
    //             vec![
    //                 " 1:11\n",
    //                 "= expected assignment, if-statement, query, or block",
    //             ],
    //         ),
    //         (
    //             "if { del(.foo) } else { del(.bar) }",
    //             vec![" 1:6\n", "= expected string"],
    //         ),
    //         (
    //             "if .foo > .bar { del(.foo) } else { .bar = .baz",
    //             // This message isn't great, ideally I'd like "expected closing bracket"
    //             vec![
    //                 " 1:48\n",
    //                 "= expected operator or path index",
    //             ],
    //         ),
    //         ("only_fields(.foo,)", vec![" 1:18\n", "= expected argument or path"]),
    //         ("only_fields(,)", vec![" 1:13\n", "= expected argument"]),
    //         (
    //             // Due to the explicit list of allowed escape chars our grammar
    //             // doesn't actually recognize this as a string literal.
    //             r#".foo = "invalid escape \k sequence""#,
    //             vec![
    //                 " 1:8\n",
    //                 "= expected assignment, if-statement, query, or block",
    //             ],
    //         ),
    //         (
    //             // Same here as above.
    //             r#".foo."invalid \k escape".sequence = "foo""#,
    //             vec![" 1:6\n", "= expected path segment"],
    //         ),
    //         (
    //             // Regexes can't be parsed as part of a path
    //             r#".foo = split(.foo, ./[aa]/)"#,
    //             vec![
    //                 " 1:27\n",
    //                 "= expected query",
    //             ],
    //         ),
    //         (
    //             // we cannot assign a regular expression to a field.
    //             r#".foo = /ab/i"#,
    //             vec!["remap error: parser error: cannot assign regex to object"],
    //         ),
    //         (
    //             // we cannot assign an array containing a regular expression to a field.
    //             r#".foo = ["ab", /ab/i]"#,
    //             vec!["remap error: parser error: cannot assign regex to object"],
    //         ),
    //         (
    //             // We cannot assign to a regular expression.
    //             r#"/ab/ = .foo"#,
    //             vec![
    //                 " 1:6\n",
    //                 "= expected operator",
    //             ],
    //         ),
    //         (
    //             r#"/ab/"#,
    //             vec!["remap error: parser error: cannot return regex from program"],
    //         ),
    //         (
    //             r#"foo = /ab/"#,
    //             vec!["remap error: parser error: cannot return regex from program"],
    //         ),
    //         (
    //             r#"[/ab/]"#,
    //             vec!["remap error: parser error: cannot return regex from program"],
    //         ),
    //         (
    //             r#"
    //                 foo = /ab/
    //                 [foo]
    //             "#,
    //             vec!["remap error: parser error: cannot return regex from program"],
    //         ),
    //         (
    //             r#"
    //                 foo = [/ab/]
    //                 foo
    //             "#,
    //             vec!["remap error: parser error: cannot return regex from program"],
    //         ),
    //         ("foo bar", vec![" 1:5\n", "= expected operator"]),
    //         ("[true] [false]", vec![" 1:8\n", "= expected operator"]),

    //         // reserved keywords
    //         ("if = true", vec![" 1:4\n", "= expected query"]),
    //         ("else = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("for = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("while = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("loop = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("abort = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("break = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("continue = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("return = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("as = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("type = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("let = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("until = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("then = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("impl = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("in = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("self = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("this = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("use = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //         ("std = true", vec![" 1:1\n", "= expected assignment, if-statement, query, or block"]),
    //     ];

    //     for (source, exp_expressions) in cases {
    //         let mut state = state::Compiler::default();
    //         let parser = Parser::new(&[], &mut state, false);
    //         let err = parser
    //             .program_from_str(source)
    //             .err()
    //             .map(|err| diagnostic::Formatter::new(source, err).to_string())
    //             .unwrap();

    //         for exp in exp_expressions {
    //             assert!(
    //                 err.contains(exp),
    //                 "expected: {}\nwith source: {}\nfull error message: {}",
    //                 exp,
    //                 source,
    //                 err
    //             );
    //         }
    //     }
    // }
}
