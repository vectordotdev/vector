use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Read},
    iter::IntoIterator,
    path::PathBuf,
};

use structopt::StructOpt;
use vector_common::TimeZone;
use vrl::{diagnostic::Formatter, state, Program, Runtime, Target, Value};

#[cfg(feature = "repl")]
use super::repl;
use super::Error;

#[derive(Debug, StructOpt)]
#[structopt(name = "VRL", about = "Vector Remap Language CLI")]
pub struct Opts {
    /// The VRL program to execute. The program ".foo = true", for example, sets the event object's
    /// `foo` field to `true`.
    #[structopt(name = "PROGRAM")]
    program: Option<String>,

    /// The file containing the event object(s) to handle. JSON events should be one per line..
    #[structopt(short, long = "input", parse(from_os_str))]
    input_file: Option<PathBuf>,

    /// The file containing the VRL program to execute. This can be used instead of `PROGRAM`.
    #[structopt(short, long = "program", conflicts_with("program"), parse(from_os_str))]
    program_file: Option<PathBuf>,

    /// Print the (modified) event object instead of the result of the final expression. Setting
    /// this flag is equivalent to using `.` as the final expression.
    #[structopt(short = "o", long)]
    print_object: bool,

    /// The timezone used to parse dates.
    #[structopt(short = "tz", long)]
    timezone: Option<String>,
}

impl Opts {
    fn timezone(&self) -> Result<TimeZone, Error> {
        if let Some(ref tz) = self.timezone {
            TimeZone::parse(tz)
                .ok_or_else(|| Error::Parse(format!("unable to parse timezone: {}", tz)))
        } else {
            Ok(TimeZone::default())
        }
    }

    fn read_program(&self) -> Result<String, Error> {
        match self.program.as_ref() {
            Some(source) => Ok(source.to_owned()),
            None => match self.program_file.as_ref() {
                Some(path) => read(File::open(path)?),
                None => Ok("".to_owned()),
            },
        }
    }

    fn read_into_objects(&self) -> Result<Vec<Value>, Error> {
        let input = match self.input_file.as_ref() {
            Some(path) => read(File::open(path)?),
            None => read(io::stdin()),
        }?;

        match input.as_str() {
            "" => Ok(vec![Value::Object(BTreeMap::default())]),
            _ => input
                .lines()
                .map(|line| Ok(serde_to_vrl(serde_json::from_str(line)?)))
                .collect::<Result<Vec<Value>, Error>>(),
        }
    }

    fn should_open_repl(&self) -> bool {
        self.program.is_none() && self.program_file.is_none()
    }
}

pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    match run(opts) {
        Ok(_) => exitcode::OK,
        Err(err) => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("{}", err);
            }
            exitcode::SOFTWARE
        }
    }
}

fn run(opts: &Opts) -> Result<(), Error> {
    let tz = opts.timezone()?;
    // Run the REPL if no program or program file is specified
    if opts.should_open_repl() {
        // If an input file is provided, use that for the REPL objects, otherwise provide a
        // generic default object.
        let repl_objects = if opts.input_file.is_some() {
            opts.read_into_objects()?
        } else {
            default_objects()
        };

        repl(repl_objects, &tz)
    } else {
        let objects = opts.read_into_objects()?;
        let source = opts.read_program()?;
        let program = vrl::compile(&source, &stdlib::all(), None).map_err(|diagnostics| {
            Error::Parse(Formatter::new(&source, diagnostics).colored().to_string())
        })?;

        for mut object in objects {
            let state = state::Runtime::default();
            let runtime = Runtime::new(state);
            let result = execute(&mut object, &program, &tz, runtime, stdlib::all()).map(|v| {
                if opts.print_object {
                    object.to_string()
                } else {
                    v.to_string()
                }
            });

            #[allow(clippy::print_stdout)]
            #[allow(clippy::print_stderr)]
            match result {
                Ok(ok) => println!("{}", ok),
                Err(err) => eprintln!("{}", err),
            }
        }

        Ok(())
    }
}

#[cfg(feature = "repl")]
fn repl(objects: Vec<Value>, timezone: &TimeZone) -> Result<(), Error> {
    repl::run(objects, timezone);
    Ok(())
}

#[cfg(not(feature = "repl"))]
fn repl(_objects: Vec<Value>, _timezone: &TimeZone) -> Result<(), Error> {
    Err(Error::ReplFeature)
}

#[cfg(not(feature = "vrl-vm"))]
fn execute(
    object: &mut impl Target,
    program: &Program,
    timezone: &TimeZone,
    mut runtime: Runtime,
    _functions: Vec<Box<dyn vrl::Function>>,
) -> Result<Value, Error> {
    runtime
        .resolve(object, program, timezone)
        .map_err(Error::Runtime)
}

#[cfg(feature = "vrl-vm")]
fn execute(
    object: &mut impl Target,
    program: &Program,
    timezone: &TimeZone,
    mut runtime: Runtime,
    functions: Vec<Box<dyn vrl::Function>>,
) -> Result<Value, Error> {
    let vm = runtime.compile(functions, program).unwrap();
    runtime
        .run_vm(&vm, object, timezone)
        .map_err(Error::Runtime)
}

fn serde_to_vrl(value: serde_json::Value) -> Value {
    use serde_json::Value;

    match value {
        Value::Null => vrl::Value::Null,
        Value::Object(v) => v
            .into_iter()
            .map(|(k, v)| (k, serde_to_vrl(v)))
            .collect::<BTreeMap<_, _>>()
            .into(),
        Value::Bool(v) => v.into(),
        Value::Number(v) if v.is_f64() => v.as_f64().unwrap().into(),
        Value::Number(v) => v.as_i64().unwrap_or(i64::MAX).into(),
        Value::String(v) => v.into(),
        Value::Array(v) => v.into_iter().map(serde_to_vrl).collect::<Vec<_>>().into(),
    }
}

fn read<R: Read>(mut reader: R) -> Result<String, Error> {
    let mut buffer = String::new();
    reader.read_to_string(&mut buffer)?;

    Ok(buffer)
}

fn default_objects() -> Vec<Value> {
    vec![Value::Object(BTreeMap::new())]
}
