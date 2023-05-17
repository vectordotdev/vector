#![feature(rustc_private)]
#![warn(unused_extern_crates)]

extern crate rustc_ast;
extern crate rustc_span;

use clippy_utils::diagnostics::span_lint;
use rustc_ast::token::{Lit, Str, Token, TokenKind};
use rustc_ast::{tokenstream::Spacing, tokenstream::TokenTree, MacCall};
use rustc_lint::{EarlyContext, EarlyLintPass};
use rustc_span::{Span, Symbol};

dylint_linting::declare_pre_expansion_lint! {
    /// ### What it does
    /// Checks for tracing log macro invocations with a misformatted message field.
    ///
    /// ### Why is this bad?
    /// We use this to enforce consistency across all of the internal logs we generate.
    ///
    /// ### Example
    /// ```rust
    /// warn!(message = "something happened");
    /// ```
    /// Use instead:
    /// ```rust
    /// warn!(message = "Something happened.");
    /// ```
    pub MISFORMATTED_TRACING_MESSAGE,
    Deny,
    "Tracing messages have incorrect format"
}

const TRACING_NAMES: [&str; 6] = ["trace", "debug", "info", "warn", "error", "event"];

impl EarlyLintPass for MisformattedTracingMessage {
    fn check_mac(&mut self, cx: &EarlyContext<'_>, mac: &MacCall) {
        let span = mac.span();
        // This is ridiculous. The filename is only present in the global context, which is
        // accessible with `with_global_context`, but the source map field is private to the
        // crate. The only way I've found to get at it is to produce the debug string for the span,
        // which leads with the filename.
        let text = format!("{span:?}");
        let filename = text.split_once(':').expect("Invalid span debug text").0;
        if filename.starts_with("lib/") || filename.starts_with("src/") {
            let name = mac.path.segments[mac.path.segments.len() - 1]
                .ident
                .as_str();
            if TRACING_NAMES.contains(&name) {
                let groups = group_token_trees(mac.args.tokens.trees());
                for group in groups {
                    let first = group[0];
                    if let Some(ident) = match_ident(first) {
                        if group.len() >= 3 && ident.as_str() == "message" && match_eq(group[1]) {
                            if let Some(symbol) = match_lit(group[2]) {
                                check_message(cx, span, symbol);
                            }
                        }
                    } else if let Some(symbol) = match_lit(first) {
                        check_message(cx, span, symbol);
                    }
                }
            }
        }
    }
}

fn check_message(cx: &EarlyContext, span: Span, message: &Symbol) {
    let message = message.as_str();
    let first = message.chars().next().expect("Empty message");
    let last = message.chars().last().expect("Empty message");
    if !first.is_uppercase() && first != '{' {
        span_lint(
            cx,
            MISFORMATTED_TRACING_MESSAGE,
            span,
            "Message must start with a capital.",
        );
    }
    if last != '.' && last != '}' {
        span_lint(
            cx,
            MISFORMATTED_TRACING_MESSAGE,
            span,
            "Message must end with a period.",
        );
    }
}

fn match_eq(tt: &TokenTree) -> bool {
    matches!(match_token(tt), Some(TokenKind::Eq))
}

fn match_ident(tt: &TokenTree) -> Option<&Symbol> {
    if let Some(TokenKind::Ident(ident, false)) = match_token(tt) {
        Some(ident)
    } else {
        None
    }
}

fn match_lit(tt: &TokenTree) -> Option<&Symbol> {
    if let Some(TokenKind::Literal(Lit {
        kind: Str, symbol, ..
    })) = match_token(tt)
    {
        Some(symbol)
    } else {
        None
    }
}

fn match_token(tt: &TokenTree) -> Option<&TokenKind> {
    if let TokenTree::Token(Token { kind, .. }, _spacing) = tt {
        Some(kind)
    } else {
        None
    }
}

fn group_token_trees<'a>(trees: impl Iterator<Item = &'a TokenTree>) -> Vec<Vec<&'a TokenTree>> {
    let mut result: Vec<Vec<_>> = vec![Vec::new()];
    for tt in trees {
        if matches!(
            tt,
            TokenTree::Token(
                Token {
                    kind: TokenKind::Comma,
                    ..
                },
                Spacing::Alone
            )
        ) {
            result.push(Vec::new());
        } else {
            result.last_mut().unwrap().push(tt);
        }
    }
    result.retain(|v| !v.is_empty());
    result
}

#[test]
fn ui() {
    dylint_misformatted_tracing_message::ui_test(
        env!("CARGO_PKG_NAME"),
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui"),
    );
}
