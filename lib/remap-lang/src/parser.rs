#![allow(clippy::or_fun_call)]

use crate::{
    expression::{
        Arithmetic, Assignment, Block, Function, IfStatement, Literal, Noop, Not, Path, Target,
    },
    Argument, Error, Expr, Operator, Result, Value,
};
use pest::iterators::{Pair, Pairs};
use regex::{Regex, RegexBuilder};
use std::str::FromStr;

#[derive(pest_derive::Parser)]
#[grammar = "../grammar.pest"]
pub(super) struct Parser;

type R = Rule;

/// Converts the set of known "root" rules into boxed [`Expression`] trait
/// objects.
pub(crate) fn pairs_to_expressions(pairs: Pairs<R>) -> Result<Vec<Expr>> {
    let mut expressions = vec![];

    for pair in pairs {
        match pair.as_rule() {
            R::assignment | R::boolean_expr | R::block | R::if_statement => {
                expressions.push(expression_from_pair(pair)?)
            }
            R::EOI => (),
            _ => return Err(e(R::expression)),
        }
    }

    Ok(expressions)
}

/// Given a `Pair`, build a boxed [`Expression`] trait object from it.
fn expression_from_pair(pair: Pair<R>) -> Result<Expr> {
    match pair.as_rule() {
        R::assignment => {
            let mut inner = pair.into_inner();
            let target = target_from_pair(inner.next().ok_or(e(R::target))?)?;
            let expression = expression_from_pair(inner.next().ok_or(e(R::expression))?)?;

            Ok(Expr::from(Assignment::new(target, Box::new(expression))))
        }
        R::boolean_expr => boolean_expr_from_pairs(pair.into_inner()),
        R::block => block_from_pairs(pair.into_inner()),
        R::if_statement => if_statement_from_pairs(pair.into_inner()),
        _ => Err(e(R::expression)),
    }
}

/// Return the target type to which a value is being assigned.
///
/// This returns a `target_path` target.
fn target_from_pair(pair: Pair<R>) -> Result<Target> {
    match pair.as_rule() {
        R::path => Ok(Target::Path(path_segments_from_pairs(pair.into_inner())?)),
        _ => Err(e(R::target)),
    }
}

/// Parse block expressions.
fn block_from_pairs(pairs: Pairs<R>) -> Result<Expr> {
    let mut expressions = vec![];

    for pair in pairs {
        expressions.push(expression_from_pair(pair)?);
    }

    Ok(Block::new(expressions).into())
}

/// Parse if-statement expressions.
fn if_statement_from_pairs(mut pairs: Pairs<R>) -> Result<Expr> {
    let conditional = expression_from_pair(pairs.next().ok_or(e(R::if_statement))?)?;
    let true_expression = expression_from_pair(pairs.next().ok_or(e(R::if_statement))?)?;
    let false_expression = pairs
        .next()
        .map(expression_from_pair)
        .transpose()?
        .unwrap_or_else(|| Expr::from(Noop));

    Ok(Expr::from(IfStatement::new(
        Box::new(conditional),
        Box::new(true_expression),
        Box::new(false_expression),
    )))
}

/// Parse not operator, or fall-through to primary values or function calls.
fn not_from_pairs(pairs: Pairs<R>) -> Result<Expr> {
    let mut count = 0;
    let mut expression = Expr::from(Noop);

    for pair in pairs {
        match pair.as_rule() {
            R::operator_not => count += 1,
            R::primary => expression = primary_from_pair(pair)?,
            R::call => expression = call_from_pair(pair)?,
            _ => return Err(e(R::not)),
        }
    }

    if count % 2 != 0 {
        expression = Expr::from(Not::new(Box::new(expression)))
    }

    Ok(expression)
}

/// Parse one of possible primary expressions.
fn primary_from_pair(pair: Pair<R>) -> Result<Expr> {
    let pair = pair.into_inner().next().ok_or(e(R::primary))?;

    match pair.as_rule() {
        R::value => value_from_pair(pair.into_inner().next().ok_or(e(R::value))?),
        R::path => path_from_pair(pair),
        R::group => expression_from_pair(pair.into_inner().next().ok_or(e(R::group))?),
        _ => Err(e(R::primary)),
    }
}

/// Parse a [`Value`] into a [`Literal`] expression.
fn value_from_pair(pair: Pair<R>) -> Result<Expr> {
    Ok(match pair.as_rule() {
        R::string => {
            let string = pair.into_inner().next().ok_or(e(R::string))?;
            Expr::from(Literal::from(escaped_string_from_pair(string)?))
        }
        R::null => Expr::from(Literal::from(Value::Null)),
        R::boolean => Expr::from(Literal::from(pair.as_str() == "true")),
        R::integer => Expr::from(Literal::from(pair.as_str().parse::<i64>().unwrap())),
        R::float => Expr::from(Literal::from(pair.as_str().parse::<f64>().unwrap())),
        _ => return Err(e(R::value)),
    })
}

/// Parse function call expressions.
fn call_from_pair(pair: Pair<R>) -> Result<Expr> {
    let mut inner = pair.into_inner();

    let ident = inner.next().ok_or(e(R::call))?.as_str().to_owned();
    let arguments = inner
        .next()
        .map(arguments_from_pair)
        .transpose()?
        .unwrap_or_default();

    Function::new(ident, arguments).map(Expr::from)
}

/// Parse into a vector of argument properties.
fn arguments_from_pair(pair: Pair<R>) -> Result<Vec<(Option<String>, Argument)>> {
    pair.into_inner()
        .map(argument_from_pair)
        .collect::<Result<_>>()
}

/// Parse optional argument keyword and [`Argument`] value.
fn argument_from_pair(pair: Pair<R>) -> Result<(Option<String>, Argument)> {
    let mut ident = None;

    for pair in pair.into_inner() {
        match pair.as_rule() {
            // This matches first, if a keyword is provided.
            R::ident => ident = Some(pair.as_str().to_owned()),
            R::regex => return Ok((ident, Argument::Regex(regex_from_pair(pair)?))),
            _ => {
                return Ok((
                    ident,
                    Argument::Expression(Box::new(expression_from_pair(pair)?)),
                ))
            }
        }
    }

    Err(e(R::argument))
}

/// Parse a [`Regex`] value
fn regex_from_pair(pair: Pair<R>) -> Result<Regex> {
    let mut inner = pair.into_inner();

    let pattern = inner.next().ok_or(e(R::regex_inner))?.as_str();
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

    RegexBuilder::new(pattern)
        .case_insensitive(i)
        .multi_line(m)
        .ignore_whitespace(x)
        .build()
        .map_err(Error::from)
}

/// Parse a [`Path`] value, e.g. ".foo.bar"
fn path_from_pair(pair: Pair<R>) -> Result<Expr> {
    let segments = path_segments_from_pairs(pair.into_inner())?;

    Ok(Expr::from(Path::new(segments)))
}

fn path_segments_from_pairs(pairs: Pairs<R>) -> Result<Vec<Vec<String>>> {
    pairs.map(path_segment_from_pair).collect::<Result<_>>()
}

fn path_segment_from_pair(pair: Pair<R>) -> Result<Vec<String>> {
    let mut segments = vec![];
    for segment in pair.into_inner() {
        match segment.as_rule() {
            R::path_field => segments.push(path_field_from_pair(segment)?),
            R::path_coalesce => segments.append(&mut path_coalesce_from_pair(segment)?),
            R::path_index => segments
                .last_mut()
                .get_or_insert(&mut "".to_owned())
                .push_str(segment.as_str()),
            _ => todo!(),
        }
    }

    Ok(segments)
}

fn path_field_from_pair(pair: Pair<R>) -> Result<String> {
    let field = pair.into_inner().next().ok_or(e(Rule::path_field))?;

    match field.as_rule() {
        R::string => {
            let string = field.into_inner().next().ok_or(e(R::string))?;
            escaped_string_from_pair(string)
        }
        R::ident => Ok(field.as_str().to_owned()),
        _ => Err(e(Rule::path_field)),
    }
}

fn path_coalesce_from_pair(pair: Pair<R>) -> Result<Vec<String>> {
    pair.into_inner()
        .map(path_field_from_pair)
        .collect::<Result<_>>()
}

fn escaped_string_from_pair(pair: Pair<R>) -> Result<String> {
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
                // This isn't reachable currently due to the explicit list of
                // allowed escape chars in our parser grammar. However, if that
                // changes then we might need to rely on this error.
                _ => return Err(Error::EscapeChar(c)),
            }
            is_escaped = false;
        } else if c == '\\' {
            is_escaped = true;
        } else {
            escaped_chars.push(c);
        }
    }

    Ok(escaped_chars.into_iter().collect())
}

// Auto-generate a set of parser functions to parse different operations.
macro_rules! operation_fns {
    (@impl $($rule:tt => { op: [$head_op:path, $($tail_op:path),+ $(,)?], next: $next:tt, })+) => (
        $(
            paste::paste! {
                fn [<$rule _from_pairs>](mut pairs: Pairs<R>) -> Result<Expr> {
                    let inner = pairs.next().ok_or(e(R::$rule))?.into_inner();
                    let mut lhs = [<$next _from_pairs>](inner)?;
                    let mut op = Operator::$head_op;

                    for pair in pairs {
                        match pair.as_rule() {
                            R::[<operator_ $rule>] => {
                                op = Operator::from_str(pair.as_str()).map_err(|_| e(R::$rule))?;
                            }
                            _ => {
                                lhs = Expr::from(Arithmetic::new(
                                    Box::new(lhs),
                                    Box::new([<$next _from_pairs>](pair.into_inner())?),
                                    op.clone(),
                                ));
                            }
                        }
                    }

                    Ok(lhs)
                }
            }
        )+
    );

    ($($rule:tt => { op: [$($op:path),+ $(,)?], next: $next:tt, })+) => (
        operation_fns!(@impl $($rule => { op: [$($op),+], next: $next, })+);
    );
}

// The operations are defined in reverse order, meaning boolean expressions are
// computed first, and multiplication last.
//
// The order of `op` operations defines operator precedence.
operation_fns! {
    multiplication => {
        op: [Multiply, Divide, Remainder],
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
        op: [And, Or],
        next: equality,
    }
}

#[inline]
fn e(rule: R) -> Error {
    Error::Rule(rule)
}
