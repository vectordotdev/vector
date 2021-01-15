use crate::Error;
use regex::Regex;
use remap::{state, Object, Program, Runtime, Value};
use remap_functions::all as funcs;
use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::{Highlighter, MatchingBracketHighlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::{self, MatchingBracketValidator, ValidationResult, Validator};
use rustyline::{Context, Editor, Helper};
use std::borrow::Cow::{self, Borrowed, Owned};

const DOCS_URL: &str = "https://vector.dev/docs/reference/remap";

const HELP_TEXT: &str = "
VRL REPL commands:
  help docs         Navigate to the VRL docs on the Vector website
  help docs <func>  Navigate to the VRL docs for the specified function
  next              Load the next object or create a new one
  prev              Load the previous object
  exit              Terminate the program
";

pub(crate) fn run(mut objects: Vec<Value>) -> Result<(), Error> {
    let mut index = 0;
    let func_docs_regex = Regex::new(r"^help\sdocs\s(\w{1,})$").unwrap();

    let mut rt = Runtime::new(state::Program::default());
    let mut rl = Editor::<Repl>::new();
    rl.set_helper(Some(Repl::new()));

    println!(
        "
> VVVVVVVV           VVVVVVVVRRRRRRRRRRRRRRRRR   LLLLLLLLLLL
> V::::::V           V::::::VR::::::::::::::::R  L:::::::::L
> V::::::V           V::::::VR::::::RRRRRR:::::R L:::::::::L
> V::::::V           V::::::VRR:::::R     R:::::RLL:::::::LL
>  V:::::V           V:::::V   R::::R     R:::::R  L:::::L
>   V:::::V         V:::::V    R::::R     R:::::R  L:::::L
>    V:::::V       V:::::V     R::::RRRRRR:::::R   L:::::L
>     V:::::V     V:::::V      R:::::::::::::RR    L:::::L
>      V:::::V   V:::::V       R::::RRRRRR:::::R   L:::::L
>       V:::::V V:::::V        R::::R     R:::::R  L:::::L
>        V:::::V:::::V         R::::R     R:::::R  L:::::L
>         V:::::::::V          R::::R     R:::::R  L:::::L         LLLLLL
>          V:::::::V         RR:::::R     R:::::RLL:::::::LLLLLLLLL:::::L
>           V:::::V          R::::::R     R:::::RL::::::::::::::::::::::L
>            V:::V           R::::::R     R:::::RL::::::::::::::::::::::L
>             VVV            RRRRRRRR     RRRRRRRLLLLLLLLLLLLLLLLLLLLLLLL
>
>                     VECTOR    REMAP    LANGUAGE
>
>
> Welcome!
>
> The CLI is running in REPL (Read-eval-print loop) mode.
>
> To run the CLI in regular mode, add a program to your command.
>
> VRL REPL commands:
>   help docs         Navigate to the VRL docs on the Vector website
>   help docs <func>  Navigate to the VRL docs for the specified function
>   next              Load the next object or create a new one
>   prev              Load the previous object
>   exit              Terminate the program
>
> Any other value is resolved to a TRL expression.
>
> Try it out now by typing `.` and hitting [enter] to see the result.\n"
    );

    loop {
        let readline = rl.readline("$ ");
        match readline.as_deref() {
            Ok(line) if line == "help" => print_help_text(),
            Ok(line) if line == "help docs" => open_url(DOCS_URL),
            Ok(line) if line == "exit" || line == "quit" => break,
            // Capture "help docs <func_name>"
            Ok(line) if func_docs_regex.is_match(line) => show_func_docs(line, &func_docs_regex),
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
                    "" => continue,
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

    let program = match Program::new(program, &remap_functions::all(), None, true) {
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

fn print_help_text() {
    println!("{}", HELP_TEXT);
}

fn open_url(url: &str) {
    if let Err(err) = webbrowser::open(url) {
        println!(
            "couldn't open default web browser: {}\n\
            you can access the desired documentation at {}",
            err, url
        );
    }
}

fn show_func_docs(line: &str, pattern: &Regex) {
    // Unwrap is okay in both cases here, as there's guaranteed to be two matches ("help docs" and
    // "help docs <func_name>")
    let matches = pattern.captures(line).unwrap();
    let func_name = matches.get(1).unwrap().as_str();

    if funcs().iter().any(|f| f.identifier() == func_name) {
        let func_url = format!("{}/#{}", DOCS_URL, func_name);
        open_url(&func_url);
    } else {
        println!("function name {} not recognized", func_name);
    }
}
