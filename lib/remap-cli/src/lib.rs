use rustyline::{Context, Editor, Helper};
use rustyline::completion::Completer;
use rustyline::highlight::{Highlighter, MatchingBracketHighlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::{self, MatchingBracketValidator, ValidationResult, Validator};
use std::borrow::Cow::{self, Borrowed, Owned};

pub mod cmd;
#[cfg(feature = "repl")]
mod repl;
#[cfg(feature = "tutorial")]
mod tutorial;

pub use cmd::{cmd, Opts};

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

    #[error("toml error: {0}")]
    Toml(#[from] toml::de::Error),

    #[cfg(not(feature = "repl"))]
    #[error("repl feature disabled, program input required")]
    ReplFeature,
}

pub struct Repl {
    highlighter: MatchingBracketHighlighter,
    validator: MatchingBracketValidator,
    hinter: HistoryHinter,
    colored_prompt: String,
}

impl Repl {
    pub fn new(prompt: &str) -> Self {
        Self {
            highlighter: MatchingBracketHighlighter::new(),
            hinter: HistoryHinter {},
            colored_prompt: prompt.to_owned(),
            validator: MatchingBracketValidator::new(),
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
        self.hinter.hint(line, pos, ctx)
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
