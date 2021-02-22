use rustyline::completion::Completer;
use rustyline::highlight::{Highlighter, MatchingBracketHighlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::{self, MatchingBracketValidator, ValidationResult, Validator};
use rustyline::{Context, Helper};
use std::borrow::Cow::{self, Borrowed, Owned};

pub mod cmd;
#[cfg(feature = "repl")]
mod repl;
#[cfg(feature = "tutorial")]
mod tutorial;

pub use cmd::{cmd, Opts};

const RESERVED_TERMS: &[&str] = &[
    "next",
    "prev",
    "exit",
    "quit",
    "help",
    "help functions",
    "help funcs",
    "help fs",
    "help docs",
];

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("parse error")]
    Parse(String),

    #[error("runtime error")]
    Runtime(String),

    #[error("json error")]
    Json(#[from] serde_json::Error),

    #[error("repl feature disabled, program input required")]
    ReplFeature,
}

pub struct Repl {
    highlighter: MatchingBracketHighlighter,
    history_hinter: HistoryHinter,
    colored_prompt: String,
    validator: MatchingBracketValidator,
    hints: Vec<&'static str>,
}

impl Repl {
    fn new(prompt: &str) -> Self {
        Self {
            highlighter: MatchingBracketHighlighter::new(),
            history_hinter: HistoryHinter {},
            colored_prompt: prompt.to_owned(),
            validator: MatchingBracketValidator::new(),
            hints: initial_hints(),
        }
    }
}

impl Helper for Repl {}
impl Completer for Repl {
    type Candidate = String;
}

impl Hinter for Repl {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<String> {
        if pos < line.len() {
            return None;
        }

        let mut hints: Vec<String> = Vec::new();

        // Add all function names to the hints
        let mut func_names = stdlib::all()
            .iter()
            .map(|f| f.identifier().into())
            .collect::<Vec<String>>();

        hints.append(&mut func_names);

        // Check history first
        if let Some(hist) = self.history_hinter.hint(line, pos, ctx) {
            return Some(hist);
        }

        // Then check the other built-in hints
        self.hints.iter().find_map(|hint| {
            if pos > 0 && hint.starts_with(&line[..pos]) {
                Some(String::from(&hint[pos..]))
            } else {
                None
            }
        })
    }
}

impl Highlighter for Repl {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        default: bool,
    ) -> Cow<'b, str> {
        if default {
            Borrowed(&self.colored_prompt)
        } else {
            Borrowed(prompt)
        }
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Owned("\x1b[1m".to_owned() + hint + "\x1b[m")
    }

    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        self.highlighter.highlight(line, pos)
    }

    fn highlight_char(&self, line: &str, pos: usize) -> bool {
        self.highlighter.highlight_char(line, pos)
    }
}

impl Validator for Repl {
    fn validate(
        &self,
        ctx: &mut validate::ValidationContext,
    ) -> rustyline::Result<ValidationResult> {
        self.validator.validate(ctx).map(|result| match result {
            ValidationResult::Valid(_) => {
                // support multi-line input by ending the line with a '\'
                if ctx.input().ends_with('\\') {
                    return ValidationResult::Incomplete;
                }

                result
            }
            result => result,
        })
    }

    fn validate_while_typing(&self) -> bool {
        self.validator.validate_while_typing()
    }
}


fn initial_hints() -> Vec<&'static str> {
    stdlib::all()
        .into_iter()
        .map(|f| f.identifier())
        .chain(RESERVED_TERMS.iter().copied())
        .collect()
}

pub fn open_url(url: &str) {
    if let Err(err) = webbrowser::open(url) {
        println!(
            "couldn't open default web browser: {}\n\
            you can access the desired documentation at {}",
            err, url
        );
    }
}
