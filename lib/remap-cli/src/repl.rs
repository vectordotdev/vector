use crate::Error;
use remap::{state, Object, Program, Runtime, Value};
use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::{Highlighter, MatchingBracketHighlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::{self, MatchingBracketValidator, ValidationResult, Validator};
use rustyline::{Context, Editor, Helper};
use std::borrow::Cow::{self, Borrowed, Owned};

pub(crate) fn run(mut objects: Vec<Value>) -> Result<(), Error> {
    let mut index = 0;

    let mut rt = Runtime::new(state::Program::default());
    let mut rl = Editor::<Repl>::new();
    rl.set_helper(Some(Repl::new()));

    println!(
        "
> TTTTTTTTTTTTTTTTTTTTTTTRRRRRRRRRRRRRRRRR   LLLLLLLLLLL
> T:::::::::::::::::::::TR::::::::::::::::R  L:::::::::L
> T:::::::::::::::::::::TR::::::RRRRRR:::::R L:::::::::L
> T:::::TT:::::::TT:::::TRR:::::R     R:::::RLL:::::::LL
> TTTTTT  T:::::T  TTTTTT  R::::R     R:::::R  L:::::L
>         T:::::T          R::::R     R:::::R  L:::::L
>         T:::::T          R::::RRRRRR:::::R   L:::::L
>         T:::::T          R:::::::::::::RR    L:::::L
>         T:::::T          R::::RRRRRR:::::R   L:::::L
>         T:::::T          R::::R     R:::::R  L:::::L
>         T:::::T          R::::R     R:::::R  L:::::L
>         T:::::T          R::::R     R:::::R  L:::::L         LLLLLL
>       TT:::::::TT      RR:::::R     R:::::RLL:::::::LLLLLLLLL:::::L
>       T:::::::::T      R::::::R     R:::::RL::::::::::::::::::::::L
>       T:::::::::T      R::::::R     R:::::RL::::::::::::::::::::::L
>       TTTTTTTTTTT      RRRRRRRR     RRRRRRRLLLLLLLLLLLLLLLLLLLLLLLL
>
>                     TIMBER    REMAP    LANGUAGE
>
>
> Welcome!
>
> The CLI is running in REPL (Read-eval-print loop) mode.
>
> To run the CLI in regular mode, add a program to your command.
>
> Type `help` to learn more.
>      `next` to load the next object, or create a new one.
>      `prev` to load the previous object.
>      `exit` to terminate the program.
>
> Any other value is resolved to a TRL expression.
>
> Try it out now by typing `.` and hitting [enter] to see the result.\n"
    );

    loop {
        let readline = rl.readline("$ ");
        match readline.as_deref() {
            Ok(line) if line == "help" => println!("You're on your own, for now."),
            Ok(line) if line == "exit" => break,
            Ok(line) if line == "quit" => break,
            Ok(line) => {
                rl.add_history_entry(line);

                let command = match line {
                    "next" => {
                        // allow adding one new object at a time
                        if index < objects.len() && objects.last() != Some(&Value::Null) {
                            index = index.saturating_add(1);
                        }

                        // add new object
                        if index == objects.len() {
                            objects.push(Value::Null)
                        }

                        "."
                    }
                    "prev" => {
                        index = index.saturating_sub(1);

                        // remove empty last object
                        if objects.last() == Some(&Value::Null) {
                            let _ = objects.pop();
                        }

                        "."
                    }
                    _ => line,
                };

                let value = resolve(objects.get_mut(index), &mut rt, command);
                println!("{}\n", value);
            }
            Err(ReadlineError::Interrupted) => break,
            Err(ReadlineError::Eof) => break,
            Err(err) => {
                println!("unable to read line: {}", err);
                break;
            }
        }
    }

    Ok(())
}

fn resolve(object: Option<&mut impl Object>, runtime: &mut Runtime, program: &str) -> String {
    let object = match object {
        None => return Value::Null.to_string(),
        Some(object) => object,
    };

    let program = match Program::new(program, &remap_functions::all(), None) {
        Ok(program) => program,
        Err(err) => return err.to_string(),
    };

    match runtime.execute(object, &program) {
        Ok(value) => value.to_string(),
        Err(err) => err.to_string(),
    }
}

struct Repl {
    highlighter: MatchingBracketHighlighter,
    validator: MatchingBracketValidator,
    hinter: HistoryHinter,
    colored_prompt: String,
}

impl Repl {
    fn new() -> Self {
        Self {
            highlighter: MatchingBracketHighlighter::new(),
            hinter: HistoryHinter {},
            colored_prompt: "$ ".to_owned(),
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
