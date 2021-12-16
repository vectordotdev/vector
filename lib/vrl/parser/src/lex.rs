use std::{fmt, iter::Peekable, str::CharIndices};

use diagnostic::{DiagnosticError, Label, Span};
use ordered_float::NotNan;

pub type Tok<'input> = Token<&'input str>;
pub type SpannedResult<'input, Loc> = Result<Spanned<'input, Loc>, Error>;
pub type Spanned<'input, Loc> = (Loc, Tok<'input>, Loc);

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error("syntax error")]
    ParseError {
        span: Span,
        source: lalrpop_util::ParseError<usize, Token<String>, String>,
        dropped_tokens: Vec<(usize, Token<String>, usize)>,
    },

    #[error("reserved keyword")]
    ReservedKeyword {
        start: usize,
        keyword: String,
        end: usize,
    },

    #[error("invalid numeric literal")]
    NumericLiteral {
        start: usize,
        error: String,
        end: usize,
    },

    #[error("invalid string literal")]
    StringLiteral { start: usize },

    #[error("invalid literal")]
    Literal { start: usize },

    #[error("invalid escape character: \\{}", .ch.unwrap_or_default())]
    EscapeChar { start: usize, ch: Option<char> },

    #[error("unexpected parse error")]
    UnexpectedParseError(String),
}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        use Error::*;

        match self {
            ParseError { source, .. } => match source {
                lalrpop_util::ParseError::InvalidToken { .. } => 200,
                lalrpop_util::ParseError::ExtraToken { .. } => 201,
                lalrpop_util::ParseError::User { .. } => 202,
                lalrpop_util::ParseError::UnrecognizedToken { .. } => 203,
                lalrpop_util::ParseError::UnrecognizedEOF { .. } => 204,
            },
            ReservedKeyword { .. } => 205,
            NumericLiteral { .. } => 206,
            StringLiteral { .. } => 207,
            Literal { .. } => 208,
            EscapeChar { .. } => 209,
            UnexpectedParseError(..) => 210,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use Error::*;

        fn update_expected(expected: Vec<String>) -> Vec<String> {
            expected
                .into_iter()
                .map(|expect| match expect.as_str() {
                    "LQuery" => r#""path literal""#.to_owned(),
                    _ => expect,
                })
                .collect::<Vec<_>>()
        }

        match self {
            ParseError { span, source, .. } => match source {
                lalrpop_util::ParseError::InvalidToken { location } => vec![Label::primary(
                    "invalid token",
                    Span::new(*location, *location + 1),
                )],
                lalrpop_util::ParseError::ExtraToken { token } => {
                    let (start, token, end) = token;
                    vec![Label::primary(
                        format!("unexpected extra token: {}", token),
                        Span::new(*start, *end),
                    )]
                }
                lalrpop_util::ParseError::User { error } => {
                    vec![Label::primary(format!("unexpected error: {}", error), span)]
                }
                lalrpop_util::ParseError::UnrecognizedToken { token, expected } => {
                    let (start, token, end) = token;
                    let span = Span::new(*start, *end);
                    let got = token.to_string();
                    let mut expected = update_expected(expected.clone());

                    // Temporary hack to improve error messages for `AnyIdent`
                    // parser rule.
                    let any_ident = [
                        r#""reserved identifier""#,
                        r#""else""#,
                        r#""false""#,
                        r#""null""#,
                        r#""true""#,
                        r#""if""#,
                    ];
                    let is_any_ident = any_ident.iter().all(|i| expected.contains(&i.to_string()));
                    if is_any_ident {
                        expected = expected
                            .into_iter()
                            .filter(|e| !any_ident.contains(&e.as_str()))
                            .collect::<Vec<_>>();
                    }

                    if token == &Token::RQuery {
                        return vec![
                            Label::primary("unexpected end of query path", span),
                            Label::context(
                                format!("expected one of: {}", expected.join(", ")),
                                span,
                            ),
                        ];
                    }

                    vec![
                        Label::primary(format!(r#"unexpected syntax token: "{}""#, got), span),
                        Label::context(format!("expected one of: {}", expected.join(", ")), span),
                    ]
                }
                lalrpop_util::ParseError::UnrecognizedEOF { location, expected } => {
                    let span = Span::new(*location, *location);
                    let expected = update_expected(expected.clone());

                    vec![
                        Label::primary("unexpected end of program", span),
                        Label::context(format!("expected one of: {}", expected.join(", ")), span),
                    ]
                }
            },

            ReservedKeyword { start, end, .. } => {
                let span = Span::new(*start, *end);

                vec![
                    Label::primary(
                        "this identifier name is reserved for future use in the language",
                        span,
                    ),
                    Label::context("use a different name instead", span),
                ]
            }

            NumericLiteral { start, error, end } => vec![Label::primary(
                format!("invalid numeric literal: {}", error),
                Span::new(*start, *end),
            )],

            StringLiteral { start } => vec![Label::primary(
                "invalid string literal",
                Span::new(*start, *start + 1),
            )],

            Literal { start } => vec![Label::primary(
                "invalid literal",
                Span::new(*start, *start + 1),
            )],

            EscapeChar { start, ch } => vec![Label::primary(
                format!(
                    "invalid escape character: {}",
                    ch.map(|ch| ch.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ),
                Span::new(*start, *start + 1),
            )],

            UnexpectedParseError(string) => vec![Label::primary(string, Span::default())],
        }
    }
}

// -----------------------------------------------------------------------------
// lexer
// -----------------------------------------------------------------------------

#[derive(Debug)]
pub struct Lexer<'input> {
    input: &'input str,
    chars: Peekable<CharIndices<'input>>,

    // state
    open_brackets: usize,
    open_braces: usize,
    open_parens: usize,

    /// Keep track of when the lexer is supposed to emit an `RQuery` token.
    ///
    /// For example:
    ///
    ///   [.foo].bar
    ///
    /// In this example, if `[` is at index `0`, then this value will contain:
    ///
    ///   [10, 5]
    ///
    /// Or:
    ///
    ///   [.foo].bar
    ///   ~~~~~~~~~~  0..10
    ///    ~~~~       1..5
    rquery_indices: Vec<usize>,
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub enum Token<S> {
    Identifier(S),
    PathField(S),
    FunctionCall(S),
    Operator(S),

    // literals
    StringLiteral(StringLiteral<S>),
    IntegerLiteral(i64),
    FloatLiteral(NotNan<f64>),
    RegexLiteral(S),
    TimestampLiteral(S),

    // Reserved for future use.
    ReservedIdentifier(S),

    // A token used by the internal parser unit tests.
    InternalTest(S),

    InvalidToken(char),

    // keywords
    If,
    Else,
    Null,
    False,
    True,
    Abort,

    // tokens
    Colon,
    Comma,
    Dot,
    LBrace,
    LBracket,
    LParen,
    Newline,
    RBrace,
    RBracket,
    RParen,
    SemiColon,
    Underscore,
    Escape,

    Equals,
    MergeEquals,
    Bang,
    Question,

    /// The {L,R}Query token is an "instruction" token. It does not represent
    /// any character in the source, instead it represents the start or end of a
    /// sequence of tokens that together form a "query".
    ///
    /// Some examples:
    ///
    /// ```text
    /// .          => LQuery, Dot, RQuery
    /// .foo       => LQuery, Dot, Ident, RQuery
    /// foo.bar[2] => LQuery, Ident, Dot, Ident, LBracket, Integer, RBracket, RQuery
    /// foo().bar  => LQuery, FunctionCall, LParen, RParen, Dot, Ident, RQuery
    /// [1].foo    => LQuery, LBracket, Integer, RBracket, Dot, Ident, RQuery
    /// { .. }[0]  => LQuery, LBrace, ..., RBrace, LBracket, ... RBracket, RQuery
    /// ```
    ///
    /// The final example shows how the lexer does not care about the semantic
    /// validity of a query (as in, getting the index from an object does not
    /// work), it only signals that one exists.
    ///
    /// Some non-matching examples:
    ///
    /// ```text
    /// . foo      => Dot, Identifier
    /// foo() .a   => FunctionCall, LParen, RParen, LQuery, Dot, Ident, RQuery
    /// [1] [2]    => RBracket, Integer, LBracket, RBracket, Integer, RBracket
    /// ```
    ///
    /// The reason these tokens exist is to allow the parser to remain
    /// whitespace-agnostic, while still being able to distinguish between the
    /// above two groups of examples.
    LQuery,
    RQuery,
}

impl<S> Token<S> {
    pub(crate) fn map<R>(self, f: impl Fn(S) -> R) -> Token<R> {
        use self::Token::*;
        match self {
            Identifier(s) => Identifier(f(s)),
            PathField(s) => PathField(f(s)),
            FunctionCall(s) => FunctionCall(f(s)),
            Operator(s) => Operator(f(s)),

            StringLiteral(s) => StringLiteral(match s {
                self::StringLiteral::Escaped(s) => self::StringLiteral::Escaped(f(s)),
                self::StringLiteral::Raw(s) => self::StringLiteral::Raw(f(s)),
            }),
            IntegerLiteral(s) => IntegerLiteral(s),
            FloatLiteral(s) => FloatLiteral(s),
            RegexLiteral(s) => RegexLiteral(f(s)),
            TimestampLiteral(s) => TimestampLiteral(f(s)),

            ReservedIdentifier(s) => ReservedIdentifier(f(s)),

            InternalTest(s) => InternalTest(f(s)),

            InvalidToken(s) => InvalidToken(s),

            Else => Else,
            False => False,
            If => If,
            Null => Null,
            True => True,
            Abort => Abort,

            // tokens
            Colon => Colon,
            Comma => Comma,
            Dot => Dot,
            LBrace => LBrace,
            LBracket => LBracket,
            LParen => LParen,
            Newline => Newline,
            RBrace => RBrace,
            RBracket => RBracket,
            RParen => RParen,
            SemiColon => SemiColon,
            Underscore => Underscore,
            Escape => Escape,

            Equals => Equals,
            MergeEquals => MergeEquals,
            Bang => Bang,
            Question => Question,

            LQuery => LQuery,
            RQuery => RQuery,
        }
    }
}

impl<S> fmt::Display for Token<S>
where
    S: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Token::*;

        let s = match *self {
            Identifier(_) => "Identifier",
            PathField(_) => "PathField",
            FunctionCall(_) => "FunctionCall",
            Operator(_) => "Operator",
            StringLiteral(_) => "StringLiteral",
            IntegerLiteral(_) => "IntegerLiteral",
            FloatLiteral(_) => "FloatLiteral",
            RegexLiteral(_) => "RegexLiteral",
            TimestampLiteral(_) => "TimestampLiteral",
            ReservedIdentifier(_) => "ReservedIdentifier",
            InternalTest(_) => "InternalTest",
            InvalidToken(_) => "InvalidToken",

            Else => "Else",
            False => "False",
            If => "If",
            Null => "Null",
            True => "True",
            Abort => "Abort",

            // tokens
            Colon => "Colon",
            Comma => "Comma",
            Dot => "Dot",
            LBrace => "LBrace",
            LBracket => "LBracket",
            LParen => "LParen",
            Newline => "Newline",
            RBrace => "RBrace",
            RBracket => "RBracket",
            RParen => "RParen",
            SemiColon => "SemiColon",
            Underscore => "Underscore",
            Escape => "Escape",

            Equals => "Equals",
            MergeEquals => "MergeEquals",
            Bang => "Bang",
            Question => "Question",

            LQuery => "LQuery",
            RQuery => "RQuery",
        };

        s.fmt(f)
    }
}

impl<'input> Token<&'input str> {
    /// Returns either a literal, reserved, or generic identifier.
    fn ident(s: &'input str) -> Self {
        use Token::*;

        match s {
            "if" => If,
            "else" => Else,
            "true" => True,
            "false" => False,
            "null" => Null,
            "abort" => Abort,

            // reserved identifiers
            "array" | "bool" | "boolean" | "break" | "continue" | "do" | "emit" | "float"
            | "for" | "forall" | "foreach" | "all" | "each" | "any" | "try" | "undefined"
            | "int" | "integer" | "iter" | "object" | "regex" | "return" | "string"
            | "traverse" | "timestamp" | "duration" | "unless" | "walk" | "while" | "loop" => {
                ReservedIdentifier(s)
            }

            _ if s.contains('@') => PathField(s),

            _ => Identifier(s),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub enum StringLiteral<S> {
    Escaped(S),
    Raw(S),
}

impl StringLiteral<&str> {
    pub fn unescape(&self) -> String {
        match self {
            StringLiteral::Escaped(s) => unescape_string_literal(s),
            StringLiteral::Raw(s) => s.to_string(),
        }
    }
}

// -----------------------------------------------------------------------------
// lexing iterator
// -----------------------------------------------------------------------------

impl<'input> Iterator for Lexer<'input> {
    type Item = SpannedResult<'input, usize>;

    fn next(&mut self) -> Option<Self::Item> {
        use Token::*;

        loop {
            let start = self.next_index();

            // Check if we need to emit a `LQuery` token.
            //
            // We don't advance the internal iterator, because this token does not
            // represent a physical character, instead it is a boundary marker.
            match self.query_start(start) {
                Err(err) => return Some(Err(err)),
                Ok(true) => {
                    // dbg!("LQuery"); // NOTE: uncomment this for debugging
                    return Some(Ok(self.token2(start, start + 1, LQuery)));
                }
                Ok(false) => (),
            }

            // Check if we need to emit a `RQuery` token.
            //
            // We don't advance the internal iterator, because this token does not
            // represent a physical character, instead it is a boundary marker.
            if let Some(pos) = self.query_end(start) {
                // dbg!("RQuery"); // NOTE: uncomment this for debugging
                return Some(Ok(self.token2(pos, pos + 1, RQuery)));
            }

            // Advance the internal iterator and emit the next token, or loop
            // again if we encounter a token we want to ignore (e.g. whitespace).
            if let Some((start, ch)) = self.bump() {
                let result = match ch {
                    '"' => Some(self.string_literal(start)),

                    ';' => Some(Ok(self.token(start, SemiColon))),
                    '\n' => Some(Ok(self.token(start, Newline))),
                    '\\' => Some(Ok(self.token(start, Escape))),

                    '(' => Some(Ok(self.open(start, LParen))),
                    '[' => Some(Ok(self.open(start, LBracket))),
                    '{' => Some(Ok(self.open(start, LBrace))),
                    '}' => Some(Ok(self.close(start, RBrace))),
                    ']' => Some(Ok(self.close(start, RBracket))),
                    ')' => Some(Ok(self.close(start, RParen))),

                    '.' => Some(Ok(self.token(start, Dot))),
                    ':' => Some(Ok(self.token(start, Colon))),
                    ',' => Some(Ok(self.token(start, Comma))),

                    '_' if !self.test_peek(is_ident_continue) => {
                        Some(Ok(self.token(start, Underscore)))
                    }

                    '?' if self.test_peek(char::is_alphabetic) => {
                        Some(Ok(self.internal_test(start)))
                    }

                    '!' if self.test_peek(|ch| ch == '!' || !is_operator(ch)) => {
                        Some(Ok(self.token(start, Bang)))
                    }

                    '#' => {
                        self.take_until(start, |ch| ch == '\n');
                        continue;
                    }

                    'r' if self.test_peek(|ch| ch == '\'') => Some(self.regex_literal(start)),
                    's' if self.test_peek(|ch| ch == '\'') => Some(self.raw_string_literal(start)),
                    't' if self.test_peek(|ch| ch == '\'') => Some(self.timestamp_literal(start)),

                    ch if is_ident_start(ch) => Some(Ok(self.identifier_or_function_call(start))),
                    ch if is_digit(ch) || (ch == '-' && self.test_peek(is_digit)) => {
                        Some(self.numeric_literal_or_identifier(start))
                    }
                    ch if is_operator(ch) => Some(Ok(self.operator(start))),
                    ch if ch.is_whitespace() => continue,

                    ch => Some(Ok(self.token(start, InvalidToken(ch)))),
                };

                // dbg!(&result); // NOTE: uncomment this for debugging

                return result;

            // If we've parsed the final character, and there are still open
            // queries, we need to keep the iterator going and close those
            // queries.
            } else if let Some(end) = self.rquery_indices.pop() {
                // dbg!("RQuery"); // NOTE: uncomment this for debugging
                return Some(Ok(self.token2(end, end + 1, RQuery)));
            }

            return None;
        }
    }
}

// -----------------------------------------------------------------------------
// lexing logic
// -----------------------------------------------------------------------------

impl<'input> Lexer<'input> {
    fn open(&mut self, start: usize, token: Token<&'input str>) -> Spanned<'input, usize> {
        match &token {
            Token::LParen => self.open_parens += 1,
            Token::LBracket => self.open_brackets += 1,
            Token::LBrace => self.open_braces += 1,
            _ => {}
        };

        self.token(start, token)
    }

    fn close(&mut self, start: usize, token: Token<&'input str>) -> Spanned<'input, usize> {
        match &token {
            Token::RParen => self.open_parens = self.open_parens.saturating_sub(1),
            Token::RBracket => self.open_brackets = self.open_brackets.saturating_sub(1),
            Token::RBrace => self.open_braces = self.open_braces.saturating_sub(1),
            _ => {}
        };

        self.token(start, token)
    }

    fn token(&mut self, start: usize, token: Token<&'input str>) -> Spanned<'input, usize> {
        let end = self.next_index();
        self.token2(start, end, token)
    }

    fn token2(
        &mut self,
        start: usize,
        end: usize,
        token: Token<&'input str>,
    ) -> Spanned<'input, usize> {
        (start, token, end)
    }

    fn query_end(&mut self, start: usize) -> Option<usize> {
        match self.rquery_indices.last() {
            Some(end) if start > 0 && start.saturating_sub(1) == *end => self.rquery_indices.pop(),
            _ => None,
        }
    }

    fn query_start(&mut self, start: usize) -> Result<bool, Error> {
        // If we already opened a query for the current position, we don't want
        // to open another one.
        if self.rquery_indices.last() == Some(&start) {
            return Ok(false);
        }

        // If the iterator is at the end, we don't want to open another one
        if self.peek().is_none() {
            return Ok(false);
        }

        // Take a clone of the existing chars iterator, to allow us to look
        // ahead without advancing the lexer's iterator. This is cheap, since
        // the original iterator only holds references.
        let mut chars = self.chars.clone();
        debug_assert!(chars.peek().is_some());

        // Only continue if the current character is a valid query start
        // character. We know there's at least one more char, given the above
        // assertion.
        if !is_query_start(chars.peek().unwrap().1) {
            return Ok(false);
        }

        // Track if the current chain is a valid one.
        //
        // A valid chain consists of a target, and a path to query that target.
        //
        // Valid examples:
        //
        //   .foo         (target = external, path = .foo)
        //   foo.bar      (target = internal, path = .bar)
        //   { .. }.bar   (target = object, path = .bar)
        //   [1][2]       (target = array, path = [2])
        //
        // Invalid examples:
        //
        //   foo          (target = internal, no path)
        //   { .. }       (target = object, no path)
        //   [1]          (target = array, no path)
        let mut valid = false;

        // Track the last char, so that we know if the next one is valid or not.
        let mut last_char = None;

        // We need to manually track for even open/close characters, to
        // determine when the span will end.
        let mut braces = 0;
        let mut brackets = 0;
        let mut parens = 0;

        let mut end = 0;
        while let Some((pos, ch)) = chars.next() {
            let take_until_end =
                |result: SpannedResult<'input, usize>,
                 last_char: &mut Option<char>,
                 end: &mut usize,
                 chars: &mut Peekable<CharIndices<'input>>| {
                    result.map(|(_, _, new)| {
                        for (i, ch) in chars {
                            *last_char = Some(ch);
                            if i == new + pos {
                                break;
                            }
                        }

                        *end = pos + new;
                    })
                };

            match ch {
                // containers
                '{' => braces += 1,
                '(' => parens += 1,
                '[' if braces == 0 && parens == 0 && brackets == 0 => {
                    brackets += 1;

                    if last_char == Some(']') {
                        valid = true
                    }

                    if last_char == Some('}') {
                        valid = true
                    }

                    if last_char == Some(')') {
                        valid = true
                    }

                    if last_char.map(is_ident_continue) == Some(true) {
                        valid = true
                    }
                }
                '[' => brackets += 1,

                // literals
                '"' => {
                    let result = Lexer::new(&self.input[pos + 1..]).string_literal(0);
                    match take_until_end(result, &mut last_char, &mut end, &mut chars) {
                        Ok(_) => continue,
                        Err(_) => break,
                    }
                }
                's' if chars.peek().map(|(_, ch)| ch) == Some(&'\'') => {
                    let result = Lexer::new(&self.input[pos + 1..]).raw_string_literal(0);
                    match take_until_end(result, &mut last_char, &mut end, &mut chars) {
                        Ok(_) => continue,
                        Err(_) => break,
                    }
                }
                'r' if chars.peek().map(|(_, ch)| ch) == Some(&'\'') => {
                    let result = Lexer::new(&self.input[pos + 1..]).regex_literal(0);
                    match take_until_end(result, &mut last_char, &mut end, &mut chars) {
                        Ok(_) => continue,
                        Err(_) => break,
                    }
                }
                't' if chars.peek().map(|(_, ch)| ch) == Some(&'\'') => {
                    let result = Lexer::new(&self.input[pos + 1..]).timestamp_literal(0);
                    match take_until_end(result, &mut last_char, &mut end, &mut chars) {
                        Ok(_) => continue,
                        Err(_) => break,
                    }
                }

                '}' if braces == 0 => break,
                '}' => braces -= 1,

                ')' if parens == 0 => break,
                ')' => parens -= 1,

                ']' if brackets == 0 => break,
                ']' => brackets -= 1,

                // the lexer doesn't care about the semantic validity inside
                // delimited regions in a query.
                _ if braces > 0 || brackets > 0 || parens > 0 => {
                    let (start_delim, end_delim) = if braces > 0 {
                        ('{', '}')
                    } else if brackets > 0 {
                        ('[', ']')
                    } else {
                        ('(', ')')
                    };

                    let mut skip_delim = 0;
                    while let Some((pos, ch)) = chars.peek() {
                        let pos = *pos;

                        let literal_check = |result: Spanned<'input, usize>, chars: &mut Peekable<CharIndices<'input>>| {
                            let (_, _, new) = result;

                            #[allow(clippy::while_let_on_iterator)]
                            while let Some((i, _)) = chars.next() {
                                if i == new + pos {
                                    break;
                                }
                            }
                            match chars.peek().map(|(_, ch)| ch) {
                                Some(ch) => Ok(*ch),
                                None => Err(()),
                            }
                        };

                        let ch = match &self.input[pos..] {
                            s if s.starts_with('"') => {
                                let r = Lexer::new(&self.input[pos + 1..]).string_literal(0)?;
                                match literal_check(r, &mut chars) {
                                    Ok(ch) => ch,
                                    Err(_) => {
                                        // The call to lexer above should have raised an appropriate error by now,
                                        // so these errors should only occur if there is a bug somewhere previously.
                                        return Err(Error::UnexpectedParseError(
                                            "Expected characters at end of string literal."
                                                .to_string(),
                                        ));
                                    }
                                }
                            }
                            s if s.starts_with("s'") => {
                                let r = Lexer::new(&self.input[pos + 1..]).raw_string_literal(0)?;
                                match literal_check(r, &mut chars) {
                                    Ok(ch) => ch,
                                    Err(_) => {
                                        return Err(Error::UnexpectedParseError(
                                            "Expected characters at end of raw string literal."
                                                .to_string(),
                                        ));
                                    }
                                }
                            }
                            s if s.starts_with("r'") => {
                                let r = Lexer::new(&self.input[pos + 1..]).regex_literal(0)?;
                                match literal_check(r, &mut chars) {
                                    Ok(ch) => ch,
                                    Err(_) => {
                                        return Err(Error::UnexpectedParseError(
                                            "Expected characters at end of regex literal."
                                                .to_string(),
                                        ));
                                    }
                                }
                            }
                            s if s.starts_with("t'") => {
                                let r = Lexer::new(&self.input[pos + 1..]).timestamp_literal(0)?;
                                match literal_check(r, &mut chars) {
                                    Ok(ch) => ch,
                                    Err(_) => {
                                        return Err(Error::UnexpectedParseError(
                                            "Expected characters at end of timestamp literal."
                                                .to_string(),
                                        ));
                                    }
                                }
                            }
                            _ => *ch,
                        };

                        if skip_delim == 0 && ch == end_delim {
                            break;
                        }
                        if let Some((_, c)) = chars.next() {
                            if c == start_delim {
                                skip_delim += 1;
                            }
                            if c == end_delim {
                                skip_delim -= 1;
                            }
                        };
                    }
                }

                '.' if last_char.is_none() => valid = true,
                '.' if last_char == Some(')') => valid = true,
                '.' if last_char == Some('}') => valid = true,
                '.' if last_char == Some(']') => valid = true,
                '.' if last_char == Some('"') => valid = true,
                '.' if last_char.map(is_ident_continue) == Some(true) => {
                    // we need to make sure we're not dealing with a float here
                    let digits = self.input[..pos]
                        .chars()
                        .rev()
                        .take_while(|ch| !ch.is_whitespace())
                        .all(|ch| is_digit(ch) || ch == '_');

                    if !digits {
                        valid = true
                    }
                }

                // function-call-abort
                '!' => {}

                // comments
                '#' => {
                    #[allow(clippy::while_let_on_iterator)]
                    while let Some((pos, ch)) = chars.next() {
                        if ch == '\n' {
                            break;
                        }

                        end = pos;
                    }
                    continue;
                }

                ch if is_ident_continue(ch) => {}

                // Any other character breaks the query chain.
                _ => break,
            }

            last_char = Some(ch);
            end = pos;
        }

        // Skip invalid query chains
        if !valid {
            return Ok(false);
        }

        // If we already tracked the current chain, we want to ignore another one.
        if self.rquery_indices.contains(&end) {
            return Ok(false);
        }

        self.rquery_indices.push(end);
        Ok(true)
    }

    fn string_literal(&mut self, start: usize) -> SpannedResult<'input, usize> {
        let content_start = self.next_index();

        loop {
            let scan_start = self.next_index();
            self.take_until(scan_start, |c| c == '"' || c == '\\');

            match self.bump() {
                Some((escape_start, '\\')) => self.escape_code(escape_start)?,
                Some((content_end, '"')) => {
                    let end = self.next_index();
                    let slice = self.slice(content_start, content_end);
                    let token = Token::StringLiteral(StringLiteral::Escaped(slice));
                    return Ok((start, token, end));
                }
                _ => break,
            };
        }

        Err(Error::StringLiteral { start })
    }

    fn regex_literal(&mut self, start: usize) -> SpannedResult<'input, usize> {
        self.quoted_literal(start, Token::RegexLiteral)
    }

    fn raw_string_literal(&mut self, start: usize) -> SpannedResult<'input, usize> {
        self.quoted_literal(start, |c| Token::StringLiteral(StringLiteral::Raw(c)))
    }

    fn timestamp_literal(&mut self, start: usize) -> SpannedResult<'input, usize> {
        self.quoted_literal(start, Token::TimestampLiteral)
    }

    fn numeric_literal_or_identifier(&mut self, start: usize) -> SpannedResult<'input, usize> {
        let (end, int) = self.take_while(start, |ch| is_digit(ch) || ch == '_');

        let negative = self.input.get(start..start + 1) == Some("-");
        match self.peek() {
            Some((_, ch)) if is_ident_continue(ch) && !negative => {
                self.bump();
                let (end, ident) = self.take_while(start, is_ident_continue);
                Ok((start, Token::ident(ident), end))
            }
            Some((_, '.')) => {
                self.bump();
                let (end, float) = self.take_while(start, |ch| is_digit(ch) || ch == '_');

                match float.replace("_", "").parse() {
                    Ok(float) => {
                        let float = NotNan::new(float).unwrap();
                        Ok((start, Token::FloatLiteral(float), end))
                    }
                    Err(err) => Err(Error::NumericLiteral {
                        start,
                        end,
                        error: err.to_string(),
                    }),
                }
            }
            None | Some(_) => match int.replace("_", "").parse() {
                Ok(int) => Ok((start, Token::IntegerLiteral(int), end)),
                Err(err) => Err(Error::NumericLiteral {
                    start,
                    end,
                    error: err.to_string(),
                }),
            },
        }
    }

    fn identifier_or_function_call(&mut self, start: usize) -> Spanned<'input, usize> {
        let (end, ident) = self.take_while(start, is_ident_continue);

        let token = if self.test_peek(|ch| ch == '(' || ch == '!') {
            Token::FunctionCall(ident)
        } else {
            Token::ident(ident)
        };

        (start, token, end)
    }

    fn operator(&mut self, start: usize) -> Spanned<'input, usize> {
        let (end, op) = self.take_while(start, is_operator);

        let token = match op {
            "=" => Token::Equals,
            "|=" => Token::MergeEquals,
            "?" => Token::Question,
            op => Token::Operator(op),
        };

        (start, token, end)
    }

    fn internal_test(&mut self, start: usize) -> Spanned<'input, usize> {
        self.bump();
        let (end, test) = self.take_while(start, char::is_alphabetic);

        (start, Token::InternalTest(test), end)
    }

    fn quoted_literal(
        &mut self,
        start: usize,
        tok: impl Fn(&'input str) -> Tok<'input>,
    ) -> SpannedResult<'input, usize> {
        self.bump();
        let content_start = self.next_index();

        loop {
            let scan_start = self.next_index();
            self.take_until(scan_start, |c| c == '\'' || c == '\\');

            match self.bump() {
                Some((_, '\\')) => self.bump(),
                Some((end, '\'')) => {
                    let content = self.slice(content_start, end);
                    let token = tok(content);
                    let end = self.next_index();

                    return Ok((start, token, end));
                }
                _ => break,
            };
        }

        Err(Error::Literal { start })
    }
}

// -----------------------------------------------------------------------------
// lexing helpers
// -----------------------------------------------------------------------------

impl<'input> Lexer<'input> {
    pub fn new(input: &'input str) -> Lexer<'input> {
        Self {
            input,
            chars: input.char_indices().peekable(),
            open_braces: 0,
            open_brackets: 0,
            open_parens: 0,
            rquery_indices: vec![],
        }
    }

    fn bump(&mut self) -> Option<(usize, char)> {
        self.chars.next()
    }

    fn peek(&mut self) -> Option<(usize, char)> {
        self.chars.peek().copied()
    }

    fn take_while<F>(&mut self, start: usize, mut keep_going: F) -> (usize, &'input str)
    where
        F: FnMut(char) -> bool,
    {
        self.take_until(start, |c| !keep_going(c))
    }

    fn take_until<F>(&mut self, start: usize, mut terminate: F) -> (usize, &'input str)
    where
        F: FnMut(char) -> bool,
    {
        while let Some((end, ch)) = self.peek() {
            if terminate(ch) {
                return (end, self.slice(start, end));
            } else {
                self.bump();
            }
        }

        let loc = self.next_index();

        (loc, self.slice(start, loc))
    }

    fn test_peek<F>(&mut self, mut test: F) -> bool
    where
        F: FnMut(char) -> bool,
    {
        self.peek().map_or(false, |(_, ch)| test(ch))
    }

    fn slice(&self, start: usize, end: usize) -> &'input str {
        &self.input[start..end]
    }

    fn next_index(&mut self) -> usize {
        self.peek().as_ref().map_or(self.input.len(), |l| l.0)
    }

    /// Returns Ok if the next char is a valid escape code.
    fn escape_code(&mut self, start: usize) -> Result<(), Error> {
        match self.bump() {
            Some((_, '\n')) => Ok(()),
            Some((_, '\'')) => Ok(()),
            Some((_, '"')) => Ok(()),
            Some((_, '\\')) => Ok(()),
            Some((_, 'n')) => Ok(()),
            Some((_, 'r')) => Ok(()),
            Some((_, 't')) => Ok(()),
            Some((start, ch)) => Err(Error::EscapeChar {
                start,
                ch: Some(ch),
            }),
            None => Err(Error::EscapeChar { start, ch: None }),
        }
    }
}

// -----------------------------------------------------------------------------
// generic helpers
// -----------------------------------------------------------------------------

fn is_ident_start(ch: char) -> bool {
    matches!(ch, '@' | '_' | 'a'..='z' | 'A'..='Z')
}

fn is_ident_continue(ch: char) -> bool {
    match ch {
        '0'..='9' => true,
        ch => is_ident_start(ch),
    }
}

fn is_query_start(ch: char) -> bool {
    match ch {
        '.' | '{' | '[' => true,
        ch => is_ident_start(ch),
    }
}

fn is_digit(ch: char) -> bool {
    ch.is_digit(10)
}

pub fn is_operator(ch: char) -> bool {
    matches!(
        ch,
        '!' | '%' | '&' | '*' | '+' | '-' | '/' | '<' | '=' | '>' | '?' | '|'
    )
}

fn unescape_string_literal(mut s: &str) -> String {
    let mut string = String::with_capacity(s.len());
    while let Some(i) = s.bytes().position(|b| b == b'\\') {
        let next = s.as_bytes()[i + 1];
        if next == b'\n' {
            // Remove the \n and any ensuing spaces or tabs
            string.push_str(&s[..i]);
            let remaining = &s[i + 2..];
            let whitespace: usize = remaining
                .chars()
                .take_while(|c| c.is_whitespace())
                .map(|c| c.len_utf8())
                .sum();
            s = &s[i + whitespace + 2..];
        } else {
            let c = match next {
                b'\'' => '\'',
                b'"' => '"',
                b'\\' => '\\',
                b'n' => '\n',
                b'r' => '\r',
                b't' => '\t',
                _ => unimplemented!("invalid escape"),
            };

            string.push_str(&s[..i]);
            string.push(c);
            s = &s[i + 2..];
        }
    }

    string.push_str(s);
    string
}

#[cfg(test)]
mod test {
    #![allow(clippy::print_stdout)] // tests

    use super::{StringLiteral, *};
    use crate::lex::Token::*;

    fn lexer(input: &str) -> impl Iterator<Item = SpannedResult<'_, usize>> + '_ {
        let mut lexer = Lexer::new(input);
        Box::new(std::iter::from_fn(move || lexer.next()))
    }

    // only exists to visually align assertions with inputs in tests
    fn data(source: &str) -> &str {
        source
    }

    fn test(input: &str, expected: Vec<(&str, Tok<'_>)>) {
        let mut lexer = lexer(input);
        let mut count = 0;
        let length = expected.len();
        for (token, (expected_span, expected_tok)) in lexer.by_ref().zip(expected.into_iter()) {
            count += 1;
            println!("{:?}", token);
            let start = expected_span.find('~').unwrap_or_default();
            let end = expected_span.rfind('~').map(|i| i + 1).unwrap_or_default();

            let expect = (start, expected_tok, end);
            assert_eq!(Ok(expect), token);
        }

        assert_eq!(count, length);
        assert!(count > 0);
        assert!(lexer.next().is_none());
    }

    #[test]
    fn unterminated_literal_errors() {
        let mut lexer = Lexer::new("a(m, r')");
        assert_eq!(Some(Err(Error::Literal { start: 0 })), lexer.next());
    }

    #[test]
    fn invalid_grok_pattern() {
        // Grok pattern has an invalid escape char -> `\]`
        let mut lexer = Lexer::new(
            r#"parse_grok!("1.2.3.4 - - [23/Mar/2021:06:46:35 +0000]", "%{IPORHOST:remote_ip} %{USER:ident} %{USER:user_name} \[%{HTTPDATE:timestamp}\]""#,
        );
        assert_eq!(
            Some(Err(Error::EscapeChar {
                start: 55,
                ch: Some('[')
            })),
            lexer.next()
        );
    }

    #[test]
    #[rustfmt::skip]
    fn string_literals() {
        use StringLiteral as S;
        use Token::StringLiteral as L;

        test(
            data(r#"foo "bar\"\n" baz "" "\t" "\"\"""#),
            vec![
                (r#"~~~                             "#, Identifier("foo")),
                (r#"    ~~~~~~~~~                   "#, L(S::Escaped("bar\\\"\\n"))),
                (r#"              ~~~               "#, Identifier("baz")),
                (r#"                  ~~            "#, L(S::Escaped(""))),
                (r#"                     ~~~~       "#, L(S::Escaped("\\t"))),
                (r#"                          ~~~~~~"#, L(S::Escaped(r#"\"\""#))),
            ],
        );
        assert_eq!(StringLiteral::Escaped(r#"\"\""#).unescape(), r#""""#);
    }

    #[test]
    #[rustfmt::skip]
    fn multiline_string_literals() {
        let mut lexer = lexer(r#""foo \
                                  bar""#);

        match lexer.next() {
            Some(Ok((_, Token::StringLiteral(s), _))) => assert_eq!("foo bar", s.unescape()),
            _ => panic!("Not a string literal"),
        }
    }

    #[test]
    fn string_literal_unexpected_escape_code() {
        assert_eq!(
            lexer(r#""\X""#).last(),
            Some(Err(Error::StringLiteral { start: 3 }))
        );
    }

    #[test]
    fn string_literal_unterminated() {
        assert_eq!(
            lexer(r#"foo "bar\"\n baz"#).last(),
            Some(Err(Error::StringLiteral { start: 4 }))
        );
    }

    #[test]
    #[rustfmt::skip]
    fn regex_literals() {
        test(
            data(r#"r'[fb]oo+' r'a/b\[rz\]' r''"#),
            vec![
                (r#"~~~~~~~~~~                 "#, RegexLiteral("[fb]oo+")),
                (r#"           ~~~~~~~~~~~~    "#, RegexLiteral("a/b\\[rz\\]")),
                (r#"                        ~~~"#, RegexLiteral("")),
            ],
        );
    }

    #[test]
    fn regex_literal_unterminated() {
        assert_eq!(
            lexer(r#"r'foo bar"#).last(),
            Some(Err(Error::Literal { start: 0 }))
        );
    }

    #[test]
    #[rustfmt::skip]
    fn timestamp_literals() {
        test(
            data(r#"t'foo \' bar'"#),
            vec![
                (r#"~~~~~~~~~~~~~"#, TimestampLiteral("foo \\' bar")),
            ],
        );
    }

    #[test]
    fn timestamp_literal_unterminated() {
        assert_eq!(
            lexer(r#"t'foo"#).last(),
            Some(Err(Error::Literal { start: 0 }))
        );
    }

    #[test]
    #[rustfmt::skip]
    fn raw_string_literals() {
        use StringLiteral as S;
        use Token::StringLiteral as L;

        test(
            data(r#"s'a "bc" \n \'d'"#),
            vec![
                (r#"~~~~~~~~~~~~~~~~"#, L(S::Raw(r#"a "bc" \n \'d"#))),
            ],
        );
    }

    #[test]
    fn raw_string_literal_unterminated() {
        assert_eq!(
            lexer(r#"s'foo"#).last(),
            Some(Err(Error::Literal { start: 0 }))
        );
    }

    #[test]
    #[rustfmt::skip]
    fn number_literals() {
        test(
            data(r#"12 012 12.43 12. 0 902.0001"#),
            vec![
                (r#"~~                         "#, IntegerLiteral(12)),
                (r#"   ~~~                     "#, IntegerLiteral(12)),
                (r#"       ~~~~~               "#, FloatLiteral(NotNan::new(12.43).unwrap())),
                (r#"             ~~~           "#, FloatLiteral(NotNan::new(12.0).unwrap())),
                (r#"                 ~         "#, IntegerLiteral(0)),
                (r#"                   ~~~~~~~~"#, FloatLiteral(NotNan::new(902.0001).unwrap())),
            ],
        );
    }

    #[test]
    #[rustfmt::skip]
    fn number_literals_underscore() {
        test(
            data(r#"1_000 1_2_3._4_0_"#),
            vec![
                (r#"~~~~~            "#, IntegerLiteral(1000)),
                (r#"      ~~~~~~~~~~~"#, FloatLiteral(NotNan::new(123.40).unwrap())),
            ],
        );
    }

    #[test]
    fn identifiers() {
        test(
            data(r#"foo bar1 if baz_12_qux else "#),
            vec![
                (r#"~~~                         "#, Identifier("foo")),
                (r#"    ~~~~                    "#, Identifier("bar1")),
                (r#"         ~~                 "#, If),
                (r#"            ~~~~~~~~~~      "#, Identifier("baz_12_qux")),
                (r#"                       ~~~~ "#, Else),
            ],
        );
    }

    #[test]
    fn function_calls() {
        test(
            data(r#"foo() bar_1() if() "#),
            vec![
                (r#"~~~                "#, FunctionCall("foo")),
                (r#"   ~               "#, LParen),
                (r#"    ~              "#, RParen),
                (r#"      ~~~~~        "#, FunctionCall("bar_1")),
                (r#"           ~       "#, LParen),
                (r#"            ~      "#, RParen),
                (r#"              ~~   "#, FunctionCall("if")),
                (r#"                ~  "#, LParen),
                (r#"                 ~ "#, RParen),
            ],
        );
    }

    #[test]
    fn single_query() {
        test(
            data(r#"."#),
            vec![
                //
                (r#"~"#, LQuery),
                (r#"~"#, Dot),
                (r#"~"#, RQuery),
            ],
        );
    }

    #[test]
    fn root_query() {
        test(
            data(r#". .foo . .bar ."#),
            vec![
                (r#"~              "#, LQuery),
                (r#"~              "#, Dot),
                (r#"~              "#, RQuery),
                (r#"  ~            "#, LQuery),
                (r#"  ~            "#, Dot),
                (r#"   ~~~         "#, Identifier("foo")),
                (r#"     ~         "#, RQuery),
                (r#"       ~       "#, LQuery),
                (r#"       ~       "#, Dot),
                (r#"       ~       "#, RQuery),
                (r#"         ~     "#, LQuery),
                (r#"         ~     "#, Dot),
                (r#"          ~~~  "#, Identifier("bar")),
                (r#"            ~  "#, RQuery),
                (r#"              ~"#, LQuery),
                (r#"              ~"#, Dot),
                (r#"              ~"#, RQuery),
            ],
        );
    }

    #[test]
    fn ampersat_in_query() {
        test(
            data(r#".@foo .bar.@ook"#),
            vec![
                (r#"~              "#, LQuery),
                (r#"~              "#, Dot),
                (r#" ~~~~          "#, PathField("@foo")),
                (r#"    ~          "#, RQuery),
                (r#"      ~        "#, LQuery),
                (r#"      ~        "#, Dot),
                (r#"       ~~~     "#, Identifier("bar")),
                (r#"          ~    "#, Dot),
                (r#"           ~~~~"#, PathField("@ook")),
                (r#"              ~"#, RQuery),
            ],
        );
    }

    #[test]
    fn queries() {
        test(
            data(r#".foo bar.baz .baz.qux"#),
            vec![
                (r#"~                    "#, LQuery),
                (r#"~                    "#, Dot),
                (r#" ~~~                 "#, Identifier("foo")),
                (r#"   ~                 "#, RQuery),
                (r#"     ~               "#, LQuery),
                (r#"     ~~~             "#, Identifier("bar")),
                (r#"        ~            "#, Dot),
                (r#"         ~~~         "#, Identifier("baz")),
                (r#"           ~         "#, RQuery),
                (r#"             ~       "#, LQuery),
                (r#"             ~       "#, Dot),
                (r#"              ~~~    "#, Identifier("baz")),
                (r#"                 ~   "#, Dot),
                (r#"                  ~~~"#, Identifier("qux")),
                (r#"                    ~"#, RQuery),
            ],
        );
    }

    #[test]
    #[rustfmt::skip]
    fn nested_queries() {
        use StringLiteral as S;
        use Token::StringLiteral as L;

        test(
            data(r#"[.foo].bar { "foo": [2][0] }"#),
            vec![
                (r#"~                           "#, LQuery),
                (r#"~                           "#, LBracket),
                (r#" ~                          "#, LQuery),
                (r#" ~                          "#, Dot),
                (r#"  ~~~                       "#, Identifier("foo")),
                (r#"    ~                       "#, RQuery),
                (r#"     ~                      "#, RBracket),
                (r#"      ~                     "#, Dot),
                (r#"       ~~~                  "#, Identifier("bar")),
                (r#"         ~                  "#, RQuery),
                (r#"           ~                "#, LBrace),
                (r#"             ~~~~~          "#, L(S::Escaped("foo"))),
                (r#"                  ~         "#, Colon),
                (r#"                    ~       "#, LQuery),
                (r#"                    ~       "#, LBracket),
                (r#"                     ~      "#, IntegerLiteral(2)),
                (r#"                      ~     "#, RBracket),
                (r#"                       ~    "#, LBracket),
                (r#"                        ~   "#, IntegerLiteral(0)),
                (r#"                         ~  "#, RBracket),
                (r#"                         ~  "#, RQuery),
                (r#"                           ~"#, RBrace),
            ],
        );
    }

    #[test]
    fn coalesced_queries() {
        test(
            data(r#".foo.(bar | baz)"#),
            vec![
                (r#"~               "#, LQuery),
                (r#"~               "#, Dot),
                (r#" ~~~            "#, Identifier("foo")),
                (r#"    ~           "#, Dot),
                (r#"     ~          "#, LParen),
                (r#"      ~~~       "#, Identifier("bar")),
                (r#"          ~     "#, Operator("|")),
                (r#"            ~~~ "#, Identifier("baz")),
                (r#"               ~"#, RParen),
                (r#"               ~"#, RQuery),
            ],
        );
    }

    #[test]
    fn complex_query_1() {
        use StringLiteral as S;
        use Token::StringLiteral as L;

        test(
            data(r#".a.(b | c  )."d\"e"[2 ][ 1]"#),
            vec![
                (r#"~                          "#, LQuery),
                (r#"~                          "#, Dot),
                (r#" ~                         "#, Identifier("a")),
                (r#"  ~                        "#, Dot),
                (r#"   ~                       "#, LParen),
                (r#"    ~                      "#, Identifier("b")),
                (r#"      ~                    "#, Operator("|")),
                (r#"        ~                  "#, Identifier("c")),
                (r#"           ~               "#, RParen),
                (r#"            ~              "#, Dot),
                (r#"             ~~~~~~        "#, L(S::Escaped("d\\\"e"))),
                (r#"                   ~       "#, LBracket),
                (r#"                    ~      "#, IntegerLiteral(2)),
                (r#"                      ~    "#, RBracket),
                (r#"                       ~   "#, LBracket),
                (r#"                         ~ "#, IntegerLiteral(1)),
                (r#"                          ~"#, RBracket),
            ],
        );
    }

    #[test]
    #[rustfmt::skip]
    fn complex_query_2() {
        use StringLiteral as S;
        use Token::StringLiteral as L;

        test(
            data(r#"{ "a": parse_json!("{ \"b\": 0 }").c }"#),
            vec![
                (r#"~                                     "#, LBrace),
                (r#"  ~~~                                 "#, L(S::Escaped("a"))),
                (r#"     ~                                "#, Colon),
                (r#"       ~                              "#, LQuery),
                (r#"       ~~~~~~~~~~                     "#, FunctionCall("parse_json")),
                (r#"                 ~                    "#, Bang),
                (r#"                  ~                   "#, LParen),
                (r#"                   ~~~~~~~~~~~~~~     "#, L(S::Escaped("{ \\\"b\\\": 0 }"))),
                (r#"                                 ~    "#, RParen),
                (r#"                                  ~   "#, Dot),
                (r#"                                   ~  "#, Identifier("c")),
                (r#"                                   ~  "#, RQuery),
                (r#"                                     ~"#, RBrace),
            ],
        );
    }

    #[test]
    #[rustfmt::skip]
    fn query_with_literals() {
        use StringLiteral as S;
        use Token::StringLiteral as L;

        test(
            data(r#"{ "a": r'b?c', "d": s'"e"\'f', "g": t'1.0T0' }.h"#),
            vec![
                (r#"~                                               "#, LQuery),
                (r#"~                                               "#, LBrace),
                (r#"  ~~~                                           "#, L(S::Escaped("a"))),
                (r#"     ~                                          "#, Colon),
                (r#"       ~~~~~~                                   "#, RegexLiteral("b?c")),
                (r#"             ~                                  "#, Comma),
                (r#"               ~~~                              "#, L(S::Escaped("d"))),
                (r#"                  ~                             "#, Colon),
                (r#"                    ~~~~~~~~~                   "#, L(S::Raw("\"e\"\\\'f"))),
                (r#"                             ~                  "#, Comma),
                (r#"                               ~~~              "#, L(S::Escaped("g"))),
                (r#"                                  ~             "#, Colon),
                (r#"                                    ~~~~~~~~    "#, TimestampLiteral("1.0T0")),
                (r#"                                             ~  "#, RBrace),
                (r#"                                              ~ "#, Dot),
                (r#"                                               ~"#, Identifier("h")),
                (r#"                                               ~"#, RQuery),
            ],
        );
    }

    #[test]
    fn variable_queries() {
        test(
            data(r#"foo.bar foo[2]"#),
            vec![
                (r#"~             "#, LQuery),
                (r#"~~~           "#, Identifier("foo")),
                (r#"   ~          "#, Dot),
                (r#"    ~~~       "#, Identifier("bar")),
                (r#"      ~       "#, RQuery),
                (r#"        ~     "#, LQuery),
                (r#"        ~~~   "#, Identifier("foo")),
                (r#"           ~  "#, LBracket),
                (r#"            ~ "#, IntegerLiteral(2)),
                (r#"             ~"#, RBracket),
                (r#"             ~"#, RQuery),
            ],
        );
    }

    #[test]
    fn object_queries() {
        use StringLiteral as S;
        use Token::StringLiteral as L;

        test(
            data(r#"{ "foo": "bar" }.foo"#),
            vec![
                (r#"~                   "#, LQuery),
                (r#"~                   "#, LBrace),
                (r#"  ~~~~~             "#, L(S::Escaped("foo"))),
                (r#"       ~            "#, Colon),
                (r#"         ~~~~~      "#, L(S::Escaped("bar"))),
                (r#"               ~    "#, RBrace),
                (r#"                ~   "#, Dot),
                (r#"                 ~~~"#, Identifier("foo")),
                (r#"                   ~"#, RQuery),
            ],
        );
    }

    #[test]
    fn array_queries() {
        test(
            data(r#"[ 1, 2 , 3].foo"#),
            vec![
                (r#"~              "#, LQuery),
                (r#"~              "#, LBracket),
                (r#"  ~            "#, IntegerLiteral(1)),
                (r#"   ~           "#, Comma),
                (r#"     ~         "#, IntegerLiteral(2)),
                (r#"       ~       "#, Comma),
                (r#"         ~     "#, IntegerLiteral(3)),
                (r#"          ~    "#, RBracket),
                (r#"           ~   "#, Dot),
                (r#"            ~~~"#, Identifier("foo")),
                (r#"              ~"#, RQuery),
            ],
        );
    }

    #[test]
    fn function_call_queries() {
        use StringLiteral as S;
        use Token::StringLiteral as L;

        test(
            data(r#"foo(ab: "c")[2].d"#),
            vec![
                (r#"~                "#, LQuery),
                (r#"~~~              "#, FunctionCall("foo")),
                (r#"   ~             "#, LParen),
                (r#"    ~~           "#, Identifier("ab")),
                (r#"      ~          "#, Colon),
                (r#"        ~~~      "#, L(S::Escaped("c"))),
                (r#"           ~     "#, RParen),
                (r#"            ~    "#, LBracket),
                (r#"             ~   "#, IntegerLiteral(2)),
                (r#"              ~  "#, RBracket),
                (r#"               ~ "#, Dot),
                (r#"                ~"#, Identifier("d")),
                (r#"                ~"#, RQuery),
            ],
        );
    }

    #[test]
    fn queries_in_array() {
        test(
            data("[foo[0]]"),
            vec![
                ("~       ", LBracket),
                (" ~      ", LQuery),
                (" ~~~    ", Identifier("foo")),
                ("    ~   ", LBracket),
                ("     ~  ", IntegerLiteral(0)),
                ("      ~ ", RBracket),
                ("      ~ ", RQuery),
                ("       ~", RBracket),
            ],
        );
    }

    #[test]
    fn queries_op() {
        test(
            data(r#".a + 3 .b == true"#),
            vec![
                (r#"~                "#, LQuery),
                (r#"~                "#, Dot),
                (r#" ~               "#, Identifier("a")),
                (r#" ~               "#, RQuery),
                (r#"   ~             "#, Operator("+")),
                (r#"     ~           "#, IntegerLiteral(3)),
                (r#"       ~         "#, LQuery),
                (r#"       ~         "#, Dot),
                (r#"        ~        "#, Identifier("b")),
                (r#"        ~        "#, RQuery),
                (r#"          ~~     "#, Operator("==")),
                (r#"             ~~~~"#, True),
            ],
        );
    }

    #[test]
    fn invalid_queries() {
        test(
            data(".foo.\n"),
            vec![
                ("~      ", LQuery),
                ("~      ", Dot),
                (" ~~~   ", Identifier("foo")),
                ("    ~  ", Dot),
                ("    ~  ", RQuery),
                ("     ~ ", Newline),
            ],
        );
    }

    #[test]
    fn queries_in_multiline() {
        test(
            data(".foo\n.bar = true"),
            vec![
                ("~               ", LQuery),
                ("~               ", Dot),
                (" ~~~            ", Identifier("foo")),
                ("   ~            ", RQuery),
                ("    ~           ", Newline),
                ("     ~          ", LQuery),
                ("     ~          ", Dot),
                ("      ~~~       ", Identifier("bar")),
                ("        ~       ", RQuery),
                ("          ~     ", Equals),
                ("            ~~~~", True),
            ],
        );
    }

    #[test]
    #[rustfmt::skip]
    fn quoted_path_queries() {
        use StringLiteral as S;
        use Token::StringLiteral as L;

        test(
            data(r#"."parent.key.with.special characters".child"#),
            vec![
                (r#"~                                          "#, LQuery),
                (r#"~                                          "#, Dot),
                (r#" ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~      "#, L(S::Escaped("parent.key.with.special characters"))),
                (r#"                                     ~     "#, Dot),
                (r#"                                      ~~~~~"#, Identifier("child")),
                (r#"                                          ~"#, RQuery),
            ],
        );
    }

    #[test]
    fn queries_digit_path() {
        test(
            data(r#".0foo foo.00_7bar.tar"#),
            vec![
                (r#"~                    "#, LQuery),
                (r#"~                    "#, Dot),
                (r#" ~~~~                "#, Identifier("0foo")),
                (r#"    ~                "#, RQuery),
                (r#"      ~              "#, LQuery),
                (r#"      ~~~            "#, Identifier("foo")),
                (r#"         ~           "#, Dot),
                (r#"          ~~~~~~~    "#, Identifier("00_7bar")),
                (r#"                 ~   "#, Dot),
                (r#"                  ~~~"#, Identifier("tar")),
                (r#"                    ~"#, RQuery),
            ],
        );
    }

    #[test]
    fn queries_nested_delims() {
        use StringLiteral as S;
        use Token::StringLiteral as L;

        test(
            data(r#"{ "foo": [true] }.foo[0]"#),
            vec![
                (r#"~                       "#, LQuery),
                (r#"~                       "#, LBrace),
                (r#"  ~~~~~                 "#, L(S::Escaped("foo"))),
                (r#"       ~                "#, Colon),
                (r#"         ~              "#, LBracket),
                (r#"          ~~~~          "#, True),
                (r#"              ~         "#, RBracket),
                (r#"                ~       "#, RBrace),
                (r#"                 ~      "#, Dot),
                (r#"                  ~~~   "#, Identifier("foo")),
                (r#"                     ~  "#, LBracket),
                (r#"                      ~ "#, IntegerLiteral(0)),
                (r#"                       ~"#, RBracket),
                (r#"                       ~"#, RQuery),
            ],
        );
    }

    #[test]
    fn queries_negative_index() {
        test(
            data("v[-1] = 2"),
            vec![
                ("~        ", LQuery),
                ("~        ", Identifier("v")),
                (" ~       ", LBracket),
                ("  ~~     ", IntegerLiteral(-1)),
                ("    ~    ", RBracket),
                ("    ~    ", RQuery),
                ("      ~  ", Equals),
                ("        ~", IntegerLiteral(2)),
            ],
        );
    }

    #[test]
    fn multi_byte_character_1() {
        use StringLiteral as S;
        use Token::StringLiteral as L;

        test(
            data("a * s'漢字' * a"),
            vec![
                ("~                ", Identifier("a")),
                ("  ~              ", Operator("*")),
                ("    ~~~~~~~~~    ", L(S::Raw("漢字"))),
                ("              ~  ", Operator("*")),
                ("                ~", Identifier("a")),
            ],
        );
    }

    #[test]
    fn multi_byte_character_2() {
        use StringLiteral as S;
        use Token::StringLiteral as L;

        test(
            data("a * s'¡' * a"),
            vec![
                ("~            ", Identifier("a")),
                ("  ~          ", Operator("*")),
                ("    ~~~~~    ", L(S::Raw("¡"))),
                ("          ~  ", Operator("*")),
                ("            ~", Identifier("a")),
            ],
        );
    }
}
