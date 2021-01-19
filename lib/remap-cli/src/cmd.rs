use super::{repl, Error};
use remap::{state, Formatter, Object, Program, Runtime, Value};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read};
use std::iter::IntoIterator;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "VRL", about = "Vector Remap Language CLI")]
pub struct Opts {
    /// The VRL program to execute. The program ".foo = true", for example, sets the event object's
    /// `foo` field to `true`.
    #[structopt(name = "PROGRAM")]
    program: Option<String>,

    /// The file containing the event object(s) to handle. The supported formats are JSON and jsonl.
    #[structopt(short, long = "input", parse(from_os_str))]
    input_file: Option<PathBuf>,

    /// The file containing the VRL program to execute. This can be used instead of `PROGRAM`.
    #[structopt(short, long = "program", conflicts_with("program"), parse(from_os_str))]
    program_file: Option<PathBuf>,

    /// Print the (modified) event object instead of the result of the final expression. Setting
    /// this flag is equivalent to using `.` as the final expression.
    #[structopt(short = "o", long)]
    print_object: bool,
}

pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    match run(opts) {
        Ok(_) => exitcode::OK,
        Err(err) => {
            eprintln!("{}", err);
            exitcode::SOFTWARE
        }
    }
}

fn run(opts: &Opts) -> Result<(), Error> {
    // Run the REPL if no program or program file is specified
    if should_open_repl(opts) {
        // If an input file is provided, use that for the REPL objects, otherwise provide a
        // generic default object.
        let repl_objects = match &opts.input_file {
            Some(file) => read_into_objects(Some(file))?,
            None => default_objects(),
        };

        repl(repl_objects)
    } else {
        let objects = read_into_objects(opts.input_file.as_ref())?;
        let program = read_program(opts.program.as_deref(), opts.program_file.as_ref())?;

        for mut object in objects {
            let result = execute(&mut object, program.clone()).map(|v| {
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

#[cfg(feature = "repl")]
fn repl(objects: Vec<Value>) -> Result<(), Error> {
    repl::run(objects)
}

#[cfg(not(feature = "repl"))]
fn repl(_: Vec<Value>) -> Result<(), Error> {
    Err(Error::ReplFeature)
}

fn execute(object: &mut impl Object, source: String) -> Result<Value, Error> {
    let state = state::Program::default();
    let mut runtime = Runtime::new(state);
    let (program, _) = Program::new(source.clone(), &remap_functions::all(), None, true).map_err(
        |diagnostics| Error::Parse(Formatter::new(&source, diagnostics).colored().to_string()),
    )?;

    runtime
        .run(object, &program)
        .map_err(|err| Error::Runtime(err.to_string()))
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

fn should_open_repl(opts: &Opts) -> bool {
    opts.program.is_none() && opts.program_file.is_none()
}

fn default_objects() -> Vec<Value> {
    vec![Value::Map(BTreeMap::new())]
}
