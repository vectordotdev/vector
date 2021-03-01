#[cfg(feature = "repl")]
mod repl;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;
use structopt::StructOpt;
use vrl::{diagnostic::Formatter, state, Runtime, Target, Value};

#[derive(Debug, StructOpt)]
#[structopt(name = "VRL", about = "Vector Remap Language CLI")]
pub struct Opts {
    /// The VRL program to execute. The program ".foo = true", for example, sets
    /// the event object's `foo` field to `true`.
    #[structopt(name = "PROGRAM")]
    program: Option<String>,

    /// The file containing the event object(s) to handle. The supported formats
    /// are JSON and jsonl.
    #[structopt(short, long = "input-file", parse(from_os_str))]
    input_file: Option<PathBuf>,

    /// The file containing the VRL program to execute. This can be used instead
    /// of `PROGRAM`.
    #[structopt(
        short,
        long = "program-file",
        conflicts_with("program"),
        parse(from_os_str)
    )]
    program_file: Option<PathBuf>,

    /// Print the (modified) event object instead of the result of the final
    /// expression. Setting this flag is equivalent to using `.` as the final
    /// expression.
    #[structopt(short = "o", long)]
    print_object: bool,
}

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

pub fn run(opts: Opts) -> Result<(), Error> {
    if opts.program.is_none() && opts.program_file.is_none() {
        repl(match opts.input_file {
            file @ Some(_) => read_into_objects(&file)?,
            None => vec![Value::Object(BTreeMap::new())],
        })
    } else {
        let objects = read_into_objects(&opts.input_file)?;
        let program = read_program(&opts.program, &opts.program_file)?;

        for mut object in objects {
            let result = resolve(&mut object, &program).map(|v| {
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

fn repl(objects: Vec<Value>) -> Result<(), Error> {
    if cfg!(feature = "repl") {
        crate::repl::run(objects);
        Ok(())
    } else {
        Err(Error::ReplFeature)
    }
}

fn resolve(object: &mut impl Target, source: &str) -> Result<Value, Error> {
    let state = state::Runtime::default();
    let mut runtime = Runtime::new(state);
    let program = vrl::compile(source, &stdlib::all()).map_err(|diagnostics| {
        Error::Parse(Formatter::new(source, diagnostics).colored().to_string())
    })?;

    runtime
        .resolve(object, &program)
        .map_err(|err| Error::Runtime(err.to_string()))
}

fn read_program(source: &Option<String>, file: &Option<PathBuf>) -> Result<String, Error> {
    match source {
        Some(source) => Ok(source.to_owned()),
        None => match file {
            Some(path) => read(File::open(path)?),
            None => Ok("".to_owned()),
        },
    }
}

fn read_into_objects(input: &Option<PathBuf>) -> Result<Vec<Value>, Error> {
    let input = match input {
        Some(path) => read(File::open(path)?),
        None => read(io::stdin()),
    }?;

    match input.as_str() {
        "" => Ok(vec![Value::Object(BTreeMap::default())]),
        _ => input
            .lines()
            .map(|line| Ok(serde_json::from_str(&line)?))
            .collect::<Result<Vec<Value>, Error>>(),
    }
}

fn read<R: Read>(mut reader: R) -> Result<String, Error> {
    let mut buffer = String::new();
    reader.read_to_string(&mut buffer)?;

    Ok(buffer)
}
