use super::Error;
use remap::{state, Object, Program, Runtime, Value};
use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::{Highlighter, MatchingBracketHighlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::{self, MatchingBracketValidator, ValidationResult, Validator};
use rustyline::{Context, Editor, Helper};
use std::borrow::Cow::{self, Borrowed, Owned};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read};
use std::iter::IntoIterator;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "VRL", about = "Vector Remap Language CLI")]
pub struct Opts {
    /// Program to execute
    ///
    /// For example, ".foo = true" will set the object's `foo` field to `true`.
    #[structopt(name = "PROGRAM")]
    program: Option<String>,

    /// File containing the object(s) to manipulate, leave empty to use stdin
    #[structopt(short, long = "input", parse(from_os_str))]
    input_file: Option<PathBuf>,

    /// File containing the program to execute, can be used instead of
    /// "PROGRAM"
    #[structopt(short, long = "program", conflicts_with("program"), parse(from_os_str))]
    program_file: Option<PathBuf>,

    /// Print the (modified) object, instead of the result of the final
    /// expression.
    ///
    /// The same result can be achieved by using `.` as the final expression.
    #[structopt(short = "o", long)]
    print_object: bool,
}

pub fn run(opts: &Opts) -> exitcode::ExitCode {
    match run_program(opts) {
        Ok(_) => exitcode::OK,
        Err(err) => {
            eprintln!("{}", err);
            exitcode::IOERR
        }
    }
}

fn run_program(opts: &Opts) -> Result<(), Error> {
    let objects = read_into_objects(opts.input_file.as_ref())?;
    let program = read_program(opts.program.as_deref(), opts.program_file.as_ref())?;

    if program.is_empty() {
        repl(objects)
    } else {
        for mut object in objects {
            let result = execute(&mut object, &program).map(|v| {
                if opts.print_object {
                    object.to_string()
                } else {
                    v.to_string()
                }
            });

            match result {
                Ok(ok) => println!("{}", ok),
                Err(err) => eprintln!("{}", err),
            }
        }

        Ok(())
    }
}

fn execute(object: &mut impl Object, program: &str) -> Result<Value, Error> {
    let state = state::Program::default();
    let mut runtime = Runtime::new(state);
    let program = Program::new(program, &remap_functions::all(), None)?;

    runtime.execute(object, &program).map_err(Into::into)
}

fn read_program(source: Option<&str>, file: Option<&PathBuf>) -> Result<String, Error> {
    match source {
        Some(source) => Ok(source.to_owned()),
        None => match file {
            Some(path) => read(File::open(path)?),
            None => Ok("".to_owned()),
        },
    }
}

fn read_into_objects(input: Option<&PathBuf>) -> Result<Vec<Value>, Error> {
    let input = match input {
        Some(path) => read(File::open(path)?),
        None => read(io::stdin()),
    }?;

    match input.as_str() {
        "" => Ok(vec![Value::Map(BTreeMap::default())]),
        _ => input
            .lines()
            .map(|line| Ok(serde_to_remap(serde_json::from_str(&line)?)))
            .collect::<Result<Vec<Value>, Error>>(),
    }
}

fn serde_to_remap(value: serde_json::Value) -> Value {
    use serde_json::Value;

    match value {
        Value::Null => remap::Value::Null,
        Value::Object(v) => v
            .into_iter()
            .map(|(k, v)| (k, serde_to_remap(v)))
            .collect::<BTreeMap<_, _>>()
            .into(),
        Value::Bool(v) => v.into(),
        Value::Number(v) if v.is_f64() => v.as_f64().unwrap().into(),
        Value::Number(v) => v.as_i64().unwrap_or(i64::MAX).into(),
        Value::String(v) => v.into(),
        Value::Array(v) => v.into_iter().map(serde_to_remap).collect::<Vec<_>>().into(),
    }
}

fn read<R: Read>(mut reader: R) -> Result<String, Error> {
    let mut buffer = String::new();
    reader.read_to_string(&mut buffer)?;

    Ok(buffer)
}

fn repl(mut objects: Vec<Value>) -> Result<(), Error> {
    let mut index = 0;

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
