use remap::{state, Object, Program, Runtime, Value};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read};
use std::iter::IntoIterator;
use std::path::PathBuf;
use structopt::StructOpt;

#[cfg(feature = "repl")]
mod repl;

#[derive(Debug, StructOpt)]
#[structopt(name = "TRL", about = "Timber Remap Language CLI")]
struct TRL {
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

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("io error")]
    Io(#[from] io::Error),

    #[error("remap error: {0}")]
    Remap(#[from] remap::RemapError),

    #[error("json error")]
    Json(#[from] serde_json::Error),

    #[cfg(not(feature = "repl"))]
    #[error("repl feature disabled, program input required")]
    ReplFeature,
}

fn main() {
    match run(TRL::from_args()) {
        Ok(_) => {}
        Err(err) => eprintln!("{}", err),
    }
}

fn run(opt: TRL) -> Result<(), Error> {
    let objects = read_into_objects(opt.input_file.as_ref())?;
    let program = read_program(opt.program.as_deref(), opt.program_file.as_ref())?;

    if program.is_empty() {
        repl(objects)
    } else {
        for mut object in objects {
            let result = execute(&mut object, &program).map(|v| {
                if opt.print_object {
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
fn repl(object: Vec<Value>) -> Result<(), Error> {
    Err(Error::ReplFeature)
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
