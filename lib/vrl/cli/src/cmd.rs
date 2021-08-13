#[cfg(feature = "repl")]
use super::repl;
use super::Error;
use shared::TimeZone;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read};
use std::iter::IntoIterator;
use std::path::PathBuf;
use structopt::StructOpt;
use vrl::{diagnostic::Formatter, state, Program, Runtime, Target, Value};

#[derive(Debug, StructOpt)]
#[structopt(name = "VRL", about = "Vector Remap Language CLI")]
pub struct Opts {
    /// The VRL program to execute. The program ".foo = true", for example, sets the event object's
    /// `foo` field to `true`.
    #[structopt(name = "PROGRAM")]
    program: Option<String>,

    /// The file containing the event object(s) to handle. JSON events should be one per line.
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
    ///
    /// Defaults to local timezone if unset.
    #[structopt(default_value = "local", short = "tz", long)]
    timezone: TimeZone,
}

impl Opts {
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
            None if termion::is_tty(&std::io::stdin()) => Ok("".to_owned()),
            None => read (io::stdin()),
        }?;

        match input.as_str() {
            "" => Ok(vec![Value::Object(BTreeMap::default())]),
            _ => input
                .lines()
                .map(|line| Ok(serde_to_vrl(serde_json::from_str(line)?)))
                .collect::<Result<Vec<Value>, Error>>(),
        }
    }
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
    if opts.program.is_none() && opts.program_file.is_none() {
        let mut tty_guard = None;

        let input = match &opts.input_file {
            Some(path) => read(File::open(path)?),
            None if !termion::is_tty(&std::io::stdin()) => {
                let data = read(io::stdin());

                // Reconnect stdin to a tty, so that we can take user input in the REPL.
                //
                // Without this, stdio points to a piped stream, which can't accept any user input
                // and instantly terminates the REPL.
                let tty = termion::get_tty()?;
                tty_guard.insert(stdio_override::StdinOverride::override_raw(tty)?);

                data
            }
            None => Ok(String::new()),
        }?;

        let objects = input
            .lines()
            .map(|line| Ok(serde_to_vrl(serde_json::from_str(line)?)))
            .collect::<Result<Vec<Value>, Error>>()?;

        return repl(objects, &opts.timezone);
    }

    let objects = opts.read_into_objects()?;
    let source = opts.read_program()?;
    let program = vrl::compile(&source, &stdlib::all()).map_err(|diagnostics| {
        Error::Parse(Formatter::new(&source, diagnostics).colored().to_string())
    })?;

    for mut object in objects {
        let result = execute(&mut object, &program, &opts.timezone).map(|v| {
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

fn repl(objects: Vec<Value>, timezone: &TimeZone) -> Result<(), Error> {
    if cfg!(feature = "repl") {
        repl::run(objects, timezone);
        Ok(())
    } else {
        Err(Error::ReplFeature)
    }
}

fn execute(
    object: &mut impl Target,
    program: &Program,
    timezone: &TimeZone,
) -> Result<Value, Error> {
    let state = state::Runtime::default();
    let mut runtime = Runtime::new(state);

    runtime
        .resolve(object, program, timezone)
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
