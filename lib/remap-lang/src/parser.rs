#![allow(clippy::or_fun_call)]

use crate::{
    expression::{
        self, Arithmetic, Array, Assignment, Block, Function, IfStatement, Literal, Noop, Not,
        Path, Target, Variable,
    },
    path, state, Error as E, Expr, Expression, Function as Fn, Operator, Result, Value,
};
use pest::iterators::{Pair, Pairs};
use regex::{Regex, RegexBuilder};
use std::str::FromStr;

#[derive(pest_derive::Parser, Default)]
#[grammar = "../grammar.pest"]
pub(super) struct Parser<'a> {
    pub function_definitions: &'a [Box<dyn Fn>],
    pub compiler_state: state::Compiler,
}

type R = Rule;

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error("cannot assign regex to object")]
    RegexAssignment,

    #[error("cannot return regex from program")]
    RegexResult,

    #[error(r#"path in variable assignment unsupported, use "${0}" without "{1}""#)]
    VariableAssignmentPath(String, String),

    #[error("regex error")]
    Regex(#[from] regex::Error),

    #[error(transparent)]
    Pest(#[from] pest::error::Error<R>),
}

// Auto-generate a set of parser functions to parse different operations.
macro_rules! operation_fns {
    (@impl $($rule:tt => { op: [$head_op:path, $($tail_op:path),+ $(,)?], next: $next:tt, })+) => (
        $(
            paste::paste! {
                fn [<$rule _from_pairs>](&mut self, mut pairs: Pairs<R>) -> Result<Expr> {
                    let inner = pairs.next().ok_or(e(R::$rule))?.into_inner();
                    let mut lhs = self.[<$next _from_pairs>](inner)?;
                    let mut op = Operator::$head_op;

                    for pair in pairs {
                        match pair.as_rule() {
                            R::[<operator_ $rule>] => {
                                op = Operator::from_str(pair.as_str()).map_err(|_| e(R::$rule))?;
                            }
                            _ => {
                                lhs = Expr::from(Arithmetic::new(
                                    Box::new(lhs),
                                    Box::new(self.[<$next _from_pairs>](pair.into_inner())?),
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

impl<'a> Parser<'a> {
    pub fn new(function_definitions: &'a [Box<dyn Fn>]) -> Self {
        Self {
            function_definitions,
            ..Default::default()
        }
    }

    pub fn program_from_str(&mut self, source: &str) -> Result<Vec<Expr>> {
        let pairs = self.pairs_from_str(R::program, source)?;
        self.pairs_to_expressions(pairs)
    }

    /// Parse a string path into a [`path::Path`] wrapper with easy access to
    /// individual path [`path::Segment`]s.
    ///
    /// This function fails if the provided path is invalid, as defined by the
    /// parser grammar.
    pub(crate) fn path_from_str(&mut self, path: &str) -> Result<path::Path> {
        let mut pairs = self.pairs_from_str(R::rule_path, path)?;
        let pair = pairs.next().ok_or(e(R::rule_path))?;

        match pair.as_rule() {
            R::path => self.path_from_pair(pair),
            _ => unreachable!(),
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
    pub(crate) fn path_field_from_str(&mut self, field: &str) -> Result<path::Field> {
        use pest::Parser;

        if let Ok(mut pairs) = Self::parse(R::rule_ident, field) {
            let field = pairs.next().ok_or(e(R::rule_ident))?.as_str().to_owned();

            return Ok(path::Field::Regular(field));
        }

        let field = self
            .pairs_from_str(R::rule_string_inner, field)?
            .next()
            .ok_or(e(R::rule_string_inner))?
            .as_str()
            .to_owned();

        Ok(path::Field::Quoted(field))
    }

    /// Converts the set of known "root" rules into boxed [`Expression`] trait
    /// objects.
    fn pairs_to_expressions(&mut self, pairs: Pairs<R>) -> Result<Vec<Expr>> {
        let mut expressions = vec![];

        for pair in pairs {
            match pair.as_rule() {
                R::assignment | R::boolean_expr | R::block | R::if_statement => {
                    expressions.push(self.expression_from_pair(pair)?)
                }
                R::EOI => (),
                _ => return Err(e(R::expression)),
            }
        }

        if let Some(expression) = expressions.last() {
            let kind = expression.type_def(&self.compiler_state).scalar_kind();

            if !kind.is_all() && kind.contains_regex() {
                return Err(Error::RegexResult.into());
            }
        }

        Ok(expressions)
    }

    fn pairs_from_str<'b>(&mut self, rule: R, s: &'b str) -> Result<Pairs<'b, R>> {
        use pest::Parser;
        Self::parse(rule, s).map_err(|err| E::from(Error::from(err)))
    }

    /// Given a `Pair`, build a boxed [`Expression`] trait object from it.
    fn expression_from_pair(&mut self, pair: Pair<R>) -> Result<Expr> {
        match pair.as_rule() {
            R::assignment => self.assignment_from_pairs(pair.into_inner()),
            R::boolean_expr => self.boolean_expr_from_pairs(pair.into_inner()),
            R::block => self.block_from_pairs(pair.into_inner()),
            R::if_statement => self.if_statement_from_pairs(pair.into_inner()),
            _ => Err(e(R::expression)),
        }
    }

    fn assignment_from_pairs(&mut self, mut pairs: Pairs<R>) -> Result<Expr> {
        let target = self.target_from_pair(pairs.next().ok_or(e(R::assignment))?)?;
        let expression = self.expression_from_pair(pairs.next().ok_or(e(R::assignment))?)?;

        // We explicitly reject assigning `Value::Regex` to an object.
        //
        // This makes it easier to implement `trait Object`, as you don't need
        // to convert `Value::Regex` to a compatible type, such as a map in
        // JSON.
        if matches!(target, Target::Path(_)) {
            match &expression {
                Expr::Literal(literal) if literal.is_regex() => {
                    return Err(Error::RegexAssignment.into())
                }
                Expr::Literal(literal) => {
                    if let Some(array) = literal.as_array() {
                        array.iter().try_for_each(|value| {
                            if value.is_regex() {
                                Err(E::from(Error::RegexAssignment))
                            } else {
                                Ok(())
                            }
                        })?
                    }
                }
                Expr::Array(array)
                    if array.expressions().iter().any(|expr| match expr {
                        Expr::Literal(literal) => literal.is_regex(),
                        _ => false,
                    }) =>
                {
                    return Err(Error::RegexAssignment.into())
                }
                _ => {}
            }
        }

        Ok(Assignment::new(target, Box::new(expression), &mut self.compiler_state).into())
    }

    /// Return the target type to which a value is being assigned.
    ///
    /// This can either return a `variable` or a `target_path` target, depending
    /// on the parser rule being processed.
    fn target_from_pair(&mut self, pair: Pair<R>) -> Result<Target> {
        match pair.as_rule() {
            R::variable => self.variable_from_pair(pair).and_then(|variable| {
                if let Some(path) = variable.path() {
                    return Err(Error::VariableAssignmentPath(
                        variable.ident().to_owned(),
                        path.to_string(),
                    )
                    .into());
                }

                Ok(Target::Variable(variable))
            }),
            R::path => Ok(Target::Path(Path::new(self.path_from_pair(pair)?))),
            _ => Err(e(R::target)),
        }
    }

    /// Parse block expressions.
    fn block_from_pairs(&mut self, pairs: Pairs<R>) -> Result<Expr> {
        let mut expressions = vec![];

        for pair in pairs {
            expressions.push(self.expression_from_pair(pair)?);
        }

        Ok(Block::new(expressions).into())
    }

    /// Parse if-statement expressions.
    fn if_statement_from_pairs(&mut self, mut pairs: Pairs<R>) -> Result<Expr> {
        // if condition
        let conditional = self.if_condition_from_pair(pairs.next().ok_or(e(R::if_statement))?)?;
        let true_expression = self.expression_from_pair(pairs.next().ok_or(e(R::if_statement))?)?;

        // else condition
        let mut false_expression = pairs
            .next_back()
            .map(|pair| self.expression_from_pair(pair))
            .transpose()?
            .unwrap_or_else(|| Expr::from(Noop));

        let mut pairs = pairs.rev().peekable();

        // optional if-else conditions
        while let Some(pair) = pairs.next() {
            let (conditional, true_expression) = match pairs.peek().map(Pair::as_rule) {
                Some(R::block) | None => {
                    let conditional = self.if_condition_from_pair(pair)?;
                    let true_expression = false_expression;
                    false_expression = Expr::from(Noop);

                    (conditional, true_expression)
                }
                Some(R::if_condition) => {
                    let next_pair = pairs.next().ok_or(e(R::if_statement))?;
                    let conditional = self.if_condition_from_pair(next_pair)?;
                    let true_expression = self.expression_from_pair(pair)?;

                    (conditional, true_expression)
                }
                _ => return Err(e(R::if_statement)),
            };

            false_expression = Expr::from(IfStatement::new(
                Box::new(conditional),
                Box::new(true_expression),
                Box::new(false_expression),
            ));
        }

        Ok(Expr::from(IfStatement::new(
            Box::new(conditional),
            Box::new(true_expression),
            Box::new(false_expression),
        )))
    }

    fn if_condition_from_pair(&mut self, pair: Pair<R>) -> Result<Expr> {
        let mut pairs = pair.into_inner();

        if let Some(R::boolean_expr) = pairs.peek().map(|p| p.as_rule()) {
            return self.expression_from_pair(pairs.next().ok_or(e(R::if_condition))?);
        }

        self.block_from_pairs(pairs)
    }

    /// Parse not operator, or fall-through to primary values or function calls.
    fn not_from_pairs(&mut self, pairs: Pairs<R>) -> Result<Expr> {
        let mut count = 0;
        let mut expression = Expr::from(Noop);

        for pair in pairs {
            match pair.as_rule() {
                R::operator_not => count += 1,
                R::primary => expression = self.primary_from_pair(pair)?,
                R::call => expression = self.call_from_pair(pair)?,
                _ => return Err(e(R::not)),
            }
        }

        if count % 2 != 0 {
            expression = Expr::from(Not::new(Box::new(expression)))
        }

        Ok(expression)
    }

    /// Parse one of possible primary expressions.
    fn primary_from_pair(&mut self, pair: Pair<R>) -> Result<Expr> {
        let pair = pair.into_inner().next().ok_or(e(R::primary))?;

        match pair.as_rule() {
            R::value => self.literal_from_pair(pair.into_inner().next().ok_or(e(R::value))?),
            R::variable => self.variable_from_pair(pair).map(Into::into),
            R::path => Ok(Path::new(self.path_from_pair(pair)?).into()),
            R::group => self.expression_from_pair(pair.into_inner().next().ok_or(e(R::group))?),
            _ => Err(e(R::primary)),
        }
    }

    /// Parse a [`Value`] into a [`Literal`] expression.
    fn literal_from_pair(&mut self, pair: Pair<R>) -> Result<Expr> {
        Ok(match pair.as_rule() {
            R::string => {
                let string = pair.into_inner().next().ok_or(e(R::string))?;
                Literal::from(self.escaped_string_from_pair(string)?).into()
            }
            R::null => Literal::from(Value::Null).into(),
            R::boolean => Literal::from(pair.as_str() == "true").into(),
            R::integer => {
                Literal::from(pair.as_str().parse::<i64>().map_err(|_| e(R::integer))?).into()
            }
            R::float => {
                Literal::from(pair.as_str().parse::<f64>().map_err(|_| e(R::float))?).into()
            }
            R::array => self.array_from_pair(pair)?.into(),
            R::regex => Literal::from(self.regex_from_pair(pair)?).into(),
            _ => return Err(e(R::value)),
        })
    }

    fn array_from_pair(&mut self, pair: Pair<R>) -> Result<Array> {
        let expressions = pair
            .into_inner()
            .map(|pair| self.expression_from_pair(pair))
            .collect::<Result<Vec<_>>>()?;

        Ok(Array::new(expressions))
    }

    /// Parse function call expressions.
    fn call_from_pair(&mut self, pair: Pair<R>) -> Result<Expr> {
        let mut inner = pair.into_inner();

        let ident = inner.next().ok_or(e(R::call))?.as_str().to_owned();
        let arguments = inner
            .next()
            .map(|pair| self.arguments_from_pair(pair))
            .transpose()?
            .unwrap_or_default();

        Function::new(ident, arguments, &self.function_definitions).map(Expr::from)
    }

    /// Parse into a vector of argument properties.
    fn arguments_from_pair(&mut self, pair: Pair<R>) -> Result<Vec<(Option<String>, Expr)>> {
        pair.into_inner()
            .map(|pair| self.argument_from_pair(pair))
            .collect::<Result<_>>()
    }

    /// Parse optional argument keyword and [`Argument`] value.
    fn argument_from_pair(&mut self, pair: Pair<R>) -> Result<(Option<String>, Expr)> {
        let mut ident = None;

        for pair in pair.into_inner() {
            match pair.as_rule() {
                // This matches first, if a keyword is provided.
                R::ident => ident = Some(pair.as_str().to_owned()),
                _ => return Ok((ident, self.expression_from_pair(pair)?)),
            }
        }

        Err(e(R::argument))
    }

    /// Parse a [`Regex`] value
    fn regex_from_pair(&self, pair: Pair<R>) -> Result<Regex> {
        let mut inner = pair.into_inner();

        let pattern = inner
            .next()
            .ok_or(e(R::regex_inner))?
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

        RegexBuilder::new(&pattern)
            .case_insensitive(i)
            .multi_line(m)
            .ignore_whitespace(x)
            .build()
            .map_err(|err| Error::from(err).into())
    }

    /// Parse a [`Path`] value, e.g. ".foo.bar"
    fn path_from_pair(&self, pair: Pair<R>) -> Result<path::Path> {
        // If no segments are provided, it's the root path (e.g. `.`).
        let path_segments = match pair.into_inner().next() {
            Some(path_segments) => path_segments,
            None => return Ok(path::Path::root()),
        };

        let segments = match path_segments.as_rule() {
            R::path_segments => self.path_segments_from_pair(path_segments)?,
            _ => return Err(e(R::path)),
        };

        Ok(path::Path::new_unchecked(segments))
    }

    fn path_segments_from_pair(&self, pair: Pair<R>) -> Result<Vec<path::Segment>> {
        pair.into_inner()
            .map(|pair| match pair.as_rule() {
                R::path_index => self.path_index_from_pair(pair),
                R::path_segment => self.path_segment_from_pair(pair),
                _ => Err(e(R::path_segments)),
            })
            .collect::<Result<_>>()
    }

    fn path_segment_from_pair(&self, pair: Pair<R>) -> Result<path::Segment> {
        let segment = pair.into_inner().next().ok_or(e(R::path_segment))?;

        match segment.as_rule() {
            R::path_field => self.path_field_segment_from_pair(segment),
            R::path_coalesce => self.path_coalesce_segment_from_pair(segment),
            _ => Err(e(R::path_segment)),
        }
    }

    fn path_field_segment_from_pair(&self, pair: Pair<R>) -> Result<path::Segment> {
        self.path_field_from_pair(pair).map(path::Segment::Field)
    }

    fn path_coalesce_segment_from_pair(&self, pair: Pair<R>) -> Result<path::Segment> {
        let fields = pair
            .into_inner()
            .map(|pair| self.path_field_from_pair(pair))
            .collect::<Result<Vec<_>>>()?;

        Ok(path::Segment::Coalesce(fields))
    }

    fn path_field_from_pair(&self, pair: Pair<R>) -> Result<path::Field> {
        let field = pair.into_inner().next().ok_or(e(Rule::path_field))?;

        match field.as_rule() {
            R::string => {
                let string = field.into_inner().next().ok_or(e(R::string))?;
                Ok(path::Field::Quoted(self.escaped_string_from_pair(string)?))
            }
            R::ident => Ok(path::Field::Regular(field.as_str().to_owned())),
            _ => Err(e(R::path_field)),
        }
    }

    fn path_index_from_pair(&self, pair: Pair<R>) -> Result<path::Segment> {
        let index = pair
            .into_inner()
            .next()
            .ok_or(e(R::path_index))?
            .as_str()
            .parse::<usize>()
            .map_err(|_| e(R::path_index_inner))?;

        Ok(path::Segment::Index(index))
    }

    /// Parse a [`Variable`] value, e.g. "$foo"
    fn variable_from_pair(&self, pair: Pair<R>) -> Result<Variable> {
        let mut inner = pair.into_inner();

        let ident = inner.next().ok_or(e(R::variable))?.as_str().to_owned();
        let segments = inner.try_fold(vec![], |mut segments, pair| {
            match pair.as_rule() {
                R::path_index => segments.push(self.path_index_from_pair(pair)?),
                R::path_segments => segments.append(&mut self.path_segments_from_pair(pair)?),
                _ => return Err(e(R::variable)),
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

        Ok(Variable::new(ident, expr))
    }

    fn escaped_string_from_pair(&self, pair: Pair<R>) -> Result<String> {
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
                    _ => return Err(e(Rule::char)),
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
            op: [And, Or],
            next: equality,
        }
    }
}

#[inline]
fn e(rule: R) -> E {
    E::Rule(rule)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RemapError;

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
            (
                "..",
                vec![" 1:2\n", "= expected path_segment"],
                Ok(vec![]),
            ),
            (
                ". bar",
                vec![" 1:3\n", "= expected EOI, assignment, if_statement, not, operator_boolean_expr, operator_equality, operator_comparison, operator_addition, operator_multiplication, or block"],
                Ok(vec![])
            ),
            (
                r#". "bar""#,
                vec![" 1:2\n", "= expected path_segment"], // TODO: improve error message
                Ok(vec![]),
            ),
        ];

        validate_rule(cases);
    }

    #[allow(clippy::type_complexity)]
    fn validate_rule(cases: Vec<(&str, Vec<&str>, Result<Vec<Expr>>)>) {
        for (mut i, (source, compile_check, run_check)) in cases.into_iter().enumerate() {
            let compile_check: Vec<&str> = compile_check;
            i += 1;

            let mut parser = Parser::new(&[]);
            let pairs = parser.program_from_str(source).map_err(RemapError::from);

            dbg!(&pairs);

            match pairs {
                Ok(got) => {
                    if compile_check.is_empty() {
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
                            err.to_string().contains(exp),
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

    #[test]
    fn check_parser_errors() {
        let cases = vec![
            (
                ".foo bar",
                vec![" 1:6\n", "= expected EOI, assignment, if_statement, not, operator_boolean_expr, operator_equality, operator_comparison, operator_addition, operator_multiplication, or block"],
            ),
            (
                ".=",
                vec![" 1:3\n", "= expected assignment, if_statement, not, or block"],
            ),
            (
                ".foo = !",
                vec![" 1:9\n", "= expected primary, operator_not, or ident"],
            ),
            (
                ".foo = to_string",
                vec![" 1:8\n", "= expected assignment, if_statement, not, or block"],
            ),
            (
                r#"foo = "bar""#,
                vec![
                    " 1:1\n",
                    "= expected assignment, if_statement, not, or block",
                ],
            ),
            (
                r#".foo.bar = "baz" and this"#,
                vec![" 1:18\n", "= expected EOI, assignment, if_statement, not, operator_boolean_expr, operator_equality, operator_comparison, operator_addition, operator_multiplication, or block"],
            ),
            (r#".foo.bar = "baz" +"#, vec![" 1:19", "= expected not"]),
            (
                ".foo.bar = .foo.(bar |)",
                vec![" 1:23\n", "= expected path_field"],
            ),
            (
                r#"if .foo > 0 { .foo = "bar" } else"#,
                vec![" 1:34\n", "= expected block"],
            ),
            (
                "if .foo { }",
                vec![
                    " 1:11\n",
                    "= expected assignment, if_statement, not, or block",
                ],
            ),
            (
                "if { del(.foo) } else { del(.bar) }",
                vec![" 1:4\n", "= expected not"],
            ),
            (
                "if .foo > .bar { del(.foo) } else { .bar = .baz",
                // This message isn't great, ideally I'd like "expected closing bracket"
                vec![" 1:48\n", "= expected assignment, if_statement, not, operator_boolean_expr, operator_equality, operator_comparison, operator_addition, operator_multiplication, path_index, or block"],
            ),
            (
                "only_fields(.foo,)",
                vec![" 1:18\n", "= expected argument"],
            ),
            (
                "only_fields(,)",
                vec![" 1:13\n", "= expected argument"],
            ),
            (
                // Due to the explicit list of allowed escape chars our grammar
                // doesn't actually recognize this as a string literal.
                r#".foo = "invalid escape \k sequence""#,
                vec![" 1:8\n", "= expected assignment, if_statement, not, or block"],
            ),
            (
                // Same here as above.
                r#".foo."invalid \k escape".sequence = "foo""#,
                vec![" 1:6\n", "= expected path_segment"],
            ),
            (
                // Regexes can't be parsed as part of a path
                r#".foo = split(.foo, ./[aa]/)"#,
                vec![" 1:23\n", "= expected assignment, if_statement, not, or block"],
            ),
            (
                // we cannot assign a regular expression to a field.
                r#".foo = /ab/i"#,
                vec!["remap error: parser error: cannot assign regex to object"],
            ),
            (
                // we cannot assign an array containing a regular expression to a field.
                r#".foo = ["ab", /ab/i]"#,
                vec!["remap error: parser error: cannot assign regex to object"],
            ),
            (
                // We cannot assign to a regular expression.
                r#"/ab/ = .foo"#,
                vec![" 1:6\n", "= expected EOI, assignment, if_statement, not, operator_boolean_expr, operator_equality, operator_comparison, operator_addition, operator_multiplication, or block"],
            ),
            (
                r#"/ab/"#,
                vec!["remap error: parser error: cannot return regex from program"],
            ),
            (
                r#"$foo = /ab/"#,
                vec!["remap error: parser error: cannot return regex from program"],
            ),
            (
                r#"[/ab/]"#,
                vec!["remap error: parser error: cannot return regex from program"],
            ),
            (
                r#"
                    $foo = /ab/
                    [$foo]
                "#,
                vec!["remap error: parser error: cannot return regex from program"],
            ),
            (
                r#"
                    $foo = [/ab/]
                    $foo
                "#,
                vec!["remap error: parser error: cannot return regex from program"],
            ),
        ];

        for (source, exp_expressions) in cases {
            let mut parser = Parser::new(&[]);
            let err = parser
                .program_from_str(source)
                .err()
                .map(RemapError::from)
                .unwrap()
                .to_string();

            for exp in exp_expressions {
                assert!(
                    err.contains(exp),
                    "expected: {}\nwith source: {}\nfull error message: {}",
                    exp,
                    source,
                    err
                );
            }
        }
    }
}
