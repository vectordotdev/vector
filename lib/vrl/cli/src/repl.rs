use std::borrow::Cow::{self, Borrowed, Owned};

use indoc::indoc;
use lazy_static::lazy_static;
use prettytable::{format, Cell, Row, Table};
use regex::Regex;
use rustyline::{
    completion::Completer,
    error::ReadlineError,
    highlight::{Highlighter, MatchingBracketHighlighter},
    hint::{Hinter, HistoryHinter},
    validate::{self, ValidationResult, Validator},
    Context, Editor, Helper,
};
use vector_common::TimeZone;
use vrl::{diagnostic::Formatter, state, value, Runtime, Target, Value};

// Create a list of all possible error values for potential docs lookup
lazy_static! {
    static ref ERRORS: Vec<String> = [
        100, 101, 102, 103, 104, 105, 106, 107, 108, 110, 203, 204, 205, 206, 207, 208, 209, 300,
        301, 302, 303, 304, 305, 306, 307, 308, 309, 310, 311, 312, 313, 314, 400, 401, 402, 403,
        601, 620, 630, 640, 650, 651, 652, 660, 701
    ]
    .iter()
    .map(|i| i.to_string())
    .collect();
}

const DOCS_URL: &str = "https://vector.dev/docs/reference/vrl";
const ERRORS_URL_ROOT: &str = "https://errors.vrl.dev";
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

pub(crate) fn run(mut objects: Vec<Value>, timezone: &TimeZone) {
    let mut index = 0;
    let func_docs_regex = Regex::new(r"^help\sdocs\s(\w{1,})$").unwrap();
    let error_docs_regex = Regex::new(r"^help\serror\s(\w{1,})$").unwrap();

    let mut compiler_state = state::Compiler::default();
    let mut rt = Runtime::new(state::Runtime::default());
    let mut rl = Editor::<Repl>::new();
    rl.set_helper(Some(Repl::new()));

    #[allow(clippy::print_stdout)]
    {
        println!("{}", BANNER_TEXT);
    }

    loop {
        let readline = rl.readline("$ ");
        match readline.as_deref() {
            Ok(line) if line == "exit" || line == "quit" => break,
            Ok(line) if line == "help" => print_help_text(),
            Ok(line) if line == "help functions" || line == "help funcs" || line == "help fs" => {
                print_function_list()
            }
            Ok(line) if line == "help docs" => open_url(DOCS_URL),
            // Capture "help error <code>"
            Ok(line) if error_docs_regex.is_match(line) => show_error_docs(line, &error_docs_regex),
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

                let result = resolve(
                    objects.get_mut(index),
                    &mut rt,
                    command,
                    &mut compiler_state,
                    timezone,
                );

                let string = match result {
                    Ok(v) => v.to_string(),
                    Err(v) => v.to_string(),
                };

                #[allow(clippy::print_stdout)]
                {
                    println!("{}\n", string);
                }
            }
            Err(ReadlineError::Interrupted) => break,
            Err(ReadlineError::Eof) => break,
            Err(err) => {
                #[allow(clippy::print_stdout)]
                {
                    println!("unable to read line: {}", err);
                }
                break;
            }
        }
    }
}

fn resolve(
    object: Option<&mut impl Target>,
    runtime: &mut Runtime,
    program: &str,
    state: &mut state::Compiler,
    timezone: &TimeZone,
) -> Result<Value, String> {
    let mut empty = value!({});
    let object = match object {
        None => &mut empty as &mut dyn Target,
        Some(object) => object,
    };

    let program = match vrl::compile_with_state(program, &stdlib::all(), state) {
        Ok(program) => program,
        Err(diagnostics) => return Err(Formatter::new(program, diagnostics).colored().to_string()),
    };

    execute(runtime, program, object, timezone)
}

#[cfg(not(feature = "vrl-vm"))]
fn execute(
    runtime: &mut Runtime,
    program: vrl::Program,
    object: &mut dyn Target,
    timezone: &TimeZone,
) -> Result<Value, String> {
    runtime
        .resolve(object, &program, timezone)
        .map_err(|err| err.to_string())
}

#[cfg(feature = "vrl-vm")]
fn execute(
    runtime: &mut Runtime,
    program: vrl::Program,
    object: &mut dyn Target,
    timezone: &TimeZone,
) -> Result<Value, String> {
    let vm = runtime.compile(stdlib::all(), &program)?;
    runtime
        .run_vm(&vm, object, timezone)
        .map_err(|err| err.to_string())
}

struct Repl {
    highlighter: MatchingBracketHighlighter,
    history_hinter: HistoryHinter,
    colored_prompt: String,
    hints: Vec<&'static str>,
}

impl Repl {
    fn new() -> Self {
        Self {
            highlighter: MatchingBracketHighlighter::new(),
            history_hinter: HistoryHinter {},
            colored_prompt: "$ ".to_owned(),
            hints: initial_hints(),
        }
    }
}

fn initial_hints() -> Vec<&'static str> {
    stdlib::all()
        .into_iter()
        .map(|f| f.identifier())
        .chain(RESERVED_TERMS.iter().copied())
        .collect()
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
        let timezone = TimeZone::default();
        let mut compiler_state = state::Compiler::default();
        let mut rt = Runtime::new(state::Runtime::default());
        let target: Option<&mut Value> = None;

        let result = match resolve(target, &mut rt, ctx.input(), &mut compiler_state, &timezone) {
            Err(error) => {
                // TODO: Ideally we'd used typed errors for this, but
                // that requires some more work to the VRL compiler.
                if error.contains("syntax error") && error.contains("unexpected end of program") {
                    ValidationResult::Incomplete
                } else {
                    ValidationResult::Valid(None)
                }
            }

            Ok(..) => ValidationResult::Valid(None),
        };

        Ok(result)
    }

    fn validate_while_typing(&self) -> bool {
        false
    }
}

fn print_function_list() {
    let table_format = *format::consts::FORMAT_NO_LINESEP_WITH_TITLE;
    let num_columns = 3;

    let mut func_table = Table::new();
    func_table.set_format(table_format);
    stdlib::all()
        .chunks(num_columns)
        .map(|funcs| {
            // Because it's possible that some chunks are only partial, e.g. have only two Some(_)
            // values when num_columns is 3, this logic below is necessary to avoid panics caused
            // by inappropriately calling funcs.get(_) on a None.
            let mut ids: Vec<Cell> = Vec::new();

            for n in 0..num_columns {
                if let Some(v) = funcs.get(n) {
                    ids.push(Cell::new(v.identifier()));
                }
            }

            func_table.add_row(Row::new(ids));
        })
        .for_each(drop);

    func_table.printstd();
}

fn print_help_text() {
    #[allow(clippy::print_stdout)]
    {
        println!("{}", HELP_TEXT);
    }
}

fn open_url(url: &str) {
    if let Err(err) = webbrowser::open(url) {
        #[allow(clippy::print_stdout)]
        {
            println!(
                "couldn't open default web browser: {}\n\
            you can access the desired documentation at {}",
                err, url
            );
        }
    }
}

fn show_func_docs(line: &str, pattern: &Regex) {
    // Unwrap is okay in both cases here, as there's guaranteed to be two matches ("help docs" and
    // "help docs <func_name>")
    let matches = pattern.captures(line).unwrap();
    let func_name = matches.get(1).unwrap().as_str();

    if stdlib::all().iter().any(|f| f.identifier() == func_name) {
        let func_url = format!("{}/functions/#{}", DOCS_URL, func_name);
        open_url(&func_url);
    } else {
        #[allow(clippy::print_stdout)]
        {
            println!("function name {} not recognized", func_name);
        }
    }
}

fn show_error_docs(line: &str, pattern: &Regex) {
    // As in show_func_docs, unwrap is okay here
    let matches = pattern.captures(line).unwrap();
    let error_code = matches.get(1).unwrap().as_str();

    if ERRORS.iter().any(|e| e == error_code) {
        let error_code_url = format!("{}/{}", ERRORS_URL_ROOT, error_code);
        open_url(&error_code_url);
    } else {
        #[allow(clippy::print_stdout)]
        {
            println!("error code {} not recognized", error_code);
        }
    }
}

const HELP_TEXT: &str = indoc! {r#"
    VRL REPL commands:
      help functions     Display a list of currently available VRL functions (aliases: ["help funcs", "help fs"])
      help docs          Navigate to the VRL docs on the Vector website
      help docs <func>   Navigate to the VRL docs for the specified function
      help error <code>  Navigate to the docs for a specific error code
      next               Load the next object or create a new one
      prev               Load the previous object
      exit               Terminate the program
"#};

const BANNER_TEXT: &str = indoc! {r#"
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
    >   help              Learn more about VRL
    >   next              Load the next object or create a new one
    >   prev              Load the previous object
    >   exit              Terminate the program
    >
    > Any other value is resolved to a VRL expression.
    >
    > Try it out now by typing `.` and hitting [enter] to see the result.
"#};
