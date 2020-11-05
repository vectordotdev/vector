#![allow(clippy::or_fun_call)]

use crate::{
    expression::{
        Arithmetic, Assignment, Block, Function, IfStatement, Literal, Noop, Not, Path, Target,
        Variable,
    },
    Argument, CompilerState, Error, Expr, Function as Fn, Operator, Result, Value,
};
use pest::iterators::{Pair, Pairs};
use regex::{Regex, RegexBuilder};
use std::str::FromStr;

#[derive(pest_derive::Parser)]
#[grammar = "../grammar.pest"]
pub(super) struct Parser<'a> {
    pub function_definitions: &'a [Box<dyn Fn>],
    pub compiler_state: CompilerState,
}

type R = Rule;

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

impl Parser<'_> {
    /// Converts the set of known "root" rules into boxed [`Expression`] trait
    /// objects.
    pub(crate) fn pairs_to_expressions(&mut self, pairs: Pairs<R>) -> Result<Vec<Expr>> {
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

        Ok(expressions)
    }

    /// Given a `Pair`, build a boxed [`Expression`] trait object from it.
    fn expression_from_pair(&mut self, pair: Pair<R>) -> Result<Expr> {
        match pair.as_rule() {
            R::assignment => {
                let mut inner = pair.into_inner();
                let target = self.target_from_pair(inner.next().ok_or(e(R::target))?)?;
                let expression =
                    self.expression_from_pair(inner.next().ok_or(e(R::expression))?)?;

                Ok(Expr::from(Assignment::new(
                    target,
                    Box::new(expression),
                    &mut self.compiler_state,
                )))
            }
            R::boolean_expr => self.boolean_expr_from_pairs(pair.into_inner()),
            R::block => self.block_from_pairs(pair.into_inner()),
            R::if_statement => self.if_statement_from_pairs(pair.into_inner()),
            _ => Err(e(R::expression)),
        }
    }

    /// Return the target type to which a value is being assigned.
    ///
    /// This can either return a `variable` or a `target_path` target, depending
    /// on the parser rule being processed.
    fn target_from_pair(&mut self, pair: Pair<R>) -> Result<Target> {
        match pair.as_rule() {
            R::variable => Ok(Target::Variable(
                pair.into_inner()
                    .next()
                    .ok_or(e(R::variable))?
                    .as_str()
                    .to_owned(),
            )),
            R::path => Ok(Target::Path(
                self.path_segments_from_pairs(pair.into_inner())?,
            )),
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
        let conditional = self.expression_from_pair(pairs.next().ok_or(e(R::if_statement))?)?;
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
                    let conditional = self.expression_from_pair(pair)?;
                    let true_expression = false_expression;
                    false_expression = Expr::from(Noop);

                    (conditional, true_expression)
                }
                Some(R::boolean_expr) => {
                    let next_pair = pairs.next().ok_or(e(R::if_statement))?;
                    let conditional = self.expression_from_pair(next_pair)?;
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
            R::value => self.value_from_pair(pair.into_inner().next().ok_or(e(R::value))?),
            R::variable => self.variable_from_pair(pair),
            R::path => self.path_from_pair(pair),
            R::group => self.expression_from_pair(pair.into_inner().next().ok_or(e(R::group))?),
            _ => Err(e(R::primary)),
        }
    }

    /// Parse a [`Value`] into a [`Literal`] expression.
    fn value_from_pair(&self, pair: Pair<R>) -> Result<Expr> {
        Ok(match pair.as_rule() {
            R::string => {
                let string = pair.into_inner().next().ok_or(e(R::string))?;
                Expr::from(Literal::from(self.escaped_string_from_pair(string)?))
            }
            R::null => Expr::from(Literal::from(Value::Null)),
            R::boolean => Expr::from(Literal::from(pair.as_str() == "true")),
            R::integer => Expr::from(Literal::from(pair.as_str().parse::<i64>().unwrap())),
            R::float => Expr::from(Literal::from(pair.as_str().parse::<f64>().unwrap())),
            _ => return Err(e(R::value)),
        })
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
    fn arguments_from_pair(&mut self, pair: Pair<R>) -> Result<Vec<(Option<String>, Argument)>> {
        pair.into_inner()
            .map(|pair| self.argument_from_pair(pair))
            .collect::<Result<_>>()
    }

    /// Parse optional argument keyword and [`Argument`] value.
    fn argument_from_pair(&mut self, pair: Pair<R>) -> Result<(Option<String>, Argument)> {
        let mut ident = None;

        for pair in pair.into_inner() {
            match pair.as_rule() {
                // This matches first, if a keyword is provided.
                R::ident => ident = Some(pair.as_str().to_owned()),
                R::regex => return Ok((ident, Argument::Regex(self.regex_from_pair(pair)?))),
                _ => {
                    return Ok((
                        ident,
                        Argument::Expression(Box::new(self.expression_from_pair(pair)?)),
                    ))
                }
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
            .map_err(Error::from)
    }

    /// Parse a [`Path`] value, e.g. ".foo.bar"
    fn path_from_pair(&self, pair: Pair<R>) -> Result<Expr> {
        let segments = self.path_segments_from_pairs(pair.into_inner())?;

        Ok(Expr::from(Path::new(segments)))
    }

    fn path_segments_from_pairs(&self, pairs: Pairs<R>) -> Result<Vec<Vec<String>>> {
        pairs
            .map(|pair| self.path_segment_from_pair(pair))
            .collect::<Result<_>>()
    }

    fn path_segment_from_pair(&self, pair: Pair<R>) -> Result<Vec<String>> {
        let mut segments = vec![];
        for segment in pair.into_inner() {
            match segment.as_rule() {
                R::path_field => segments.push(self.path_field_from_pair(segment)?),
                R::path_coalesce => segments.append(&mut self.path_coalesce_from_pair(segment)?),
                R::path_index => segments
                    .last_mut()
                    .get_or_insert(&mut "".to_owned())
                    .push_str(segment.as_str()),
                _ => todo!(),
            }
        }

        Ok(segments)
    }

    fn path_field_from_pair(&self, pair: Pair<R>) -> Result<String> {
        let field = pair.into_inner().next().ok_or(e(Rule::path_field))?;

        match field.as_rule() {
            R::string => {
                let string = field.into_inner().next().ok_or(e(R::string))?;
                self.escaped_string_from_pair(string)
            }
            R::ident => Ok(field.as_str().to_owned()),
            _ => Err(e(Rule::path_field)),
        }
    }

    fn path_coalesce_from_pair(&self, pair: Pair<R>) -> Result<Vec<String>> {
        pair.into_inner()
            .map(|pair| self.path_field_from_pair(pair))
            .collect::<Result<_>>()
    }

    /// Parse a [`Variable`] value, e.g. "$foo"
    fn variable_from_pair(&self, pair: Pair<R>) -> Result<Expr> {
        let ident = pair.into_inner().next().ok_or(e(R::variable))?;

        Ok(Expr::from(Variable::new(ident.as_str().to_owned())))
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
}

#[inline]
fn e(rule: R) -> Error {
    Error::Rule(rule)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pest::Parser as _;

    #[test]
    fn check_parser_errors() {
        let cases = vec![
            (r#". = "bar""#, vec![" 1:2\n", "= expected path_segment"]),
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
                    "= expected EOI, assignment, if_statement, not, or block",
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
                vec![" 1:48\n", "= expected operator_boolean_expr, operator_equality, operator_comparison, operator_addition, operator_multiplication, or path_index"],
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
                vec![" 1:21\n", "= expected path_segment"],
            ),
            (
                // We cannot assign a regular expression to a field.
                r#".foo = /ab/i"#,
                vec![" 1:8\n", "= expected assignment, if_statement, not, or block"],
            ),
            (
                // We cannot assign to a regular expression.
                r#"/ab/ = .foo"#,
                vec![" 1:1\n", "= expected EOI, assignment, if_statement, not, or block"],
            ),
        ];

        for (source, exp_expressions) in cases {
            let err = Parser::parse(Rule::program, source)
                .err()
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
