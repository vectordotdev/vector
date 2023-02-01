use core::TargetValueRef;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Read},
    iter::IntoIterator,
    path::PathBuf,
};

use ::value::Value;
use clap::Parser;
use lookup::{owned_value_path, OwnedTargetPath};
use value::Secrets;
use vector_common::TimeZone;
use vrl::state::TypeState;
use vrl::{diagnostic::Formatter, state, Program, Runtime, Target, VrlRuntime};
use vrl::{CompilationResult, CompileConfig};

#[cfg(feature = "repl")]
use super::repl;
use super::Error;

#[derive(Parser, Debug)]
#[command(name = "VRL", about = "Vector Remap Language CLI")]
pub struct Opts {
    /// The VRL program to execute. The program ".foo = true", for example, sets the event object's
    /// `foo` field to `true`.
    #[arg(id = "PROGRAM")]
    program: Option<String>,

    /// The file containing the event object(s) to handle. JSON events should be one per line..
    #[arg(short, long = "input")]
    input_file: Option<PathBuf>,

    /// The file containing the VRL program to execute. This can be used instead of `PROGRAM`.
    #[arg(short, long = "program", conflicts_with("PROGRAM"))]
    program_file: Option<PathBuf>,

    /// Print the (modified) event object instead of the result of the final expression. Setting
    /// this flag is equivalent to using `.` as the final expression.
    #[arg(short = 'o', long)]
    print_object: bool,

    /// The timezone used to parse dates.
    #[arg(short = 'z', long)]
    timezone: Option<String>,

    /// Should we use the VM to evaluate the VRL
    #[arg(short, long = "runtime", default_value_t)]
    runtime: VrlRuntime,

    // Should the CLI emit warnings
    #[arg(long = "print-warnings")]
    print_warnings: bool,
}

impl Opts {
    fn timezone(&self) -> Result<TimeZone, Error> {
        if let Some(ref tz) = self.timezone {
            TimeZone::parse(tz)
                .ok_or_else(|| Error::Parse(format!("unable to parse timezone: {tz}")))
        } else {
            Ok(TimeZone::default())
        }
    }

    fn read_program(&self) -> Result<String, Error> {
        match self.program.as_ref() {
            Some(source) => Ok(source.clone()),
            None => match self.program_file.as_ref() {
                Some(path) => read(File::open(path)?),
                None => Ok(String::new()),
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

#[must_use]
pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    match run(opts) {
        Ok(_) => exitcode::OK,
        Err(err) => {
            #[allow(clippy::print_stderr)]
            {
                eprintln!("{err}");
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

        repl(repl_objects, tz, opts.runtime)
    } else {
        let objects = opts.read_into_objects()?;
        let source = opts.read_program()?;

        // The CLI should be moved out of the "vrl" module, and then it can use the `vector-core::compile_vrl` function which includes this automatically
        let mut config = CompileConfig::default();
        config.set_read_only_path(OwnedTargetPath::metadata(owned_value_path!("vector")), true);

        let state = TypeState::default();

        let CompilationResult {
            program,
            warnings,
            config: _,
        } = vrl::compile_with_state(&source, &stdlib::all(), &state, CompileConfig::default())
            .map_err(|diagnostics| {
                Error::Parse(Formatter::new(&source, diagnostics).colored().to_string())
            })?;

        #[allow(clippy::print_stderr)]
        if opts.print_warnings {
            let warnings = Formatter::new(&source, warnings).colored().to_string();
            eprintln!("{warnings}")
        }

        for mut object in objects {
            let mut metadata = Value::Object(BTreeMap::new());
            let mut secrets = Secrets::new();
            let mut target = TargetValueRef {
                value: &mut object,
                metadata: &mut metadata,
                secrets: &mut secrets,
            };
            let state = state::Runtime::default();
            let runtime = Runtime::new(state);

            let result = execute(&mut target, &program, tz, runtime, opts.runtime).map(|v| {
                if opts.print_object {
                    object.to_string()
                } else {
                    v.to_string()
                }
            });

            #[allow(clippy::print_stdout)]
            #[allow(clippy::print_stderr)]
            match result {
                Ok(ok) => println!("{ok}"),
                Err(err) => eprintln!("{err}"),
            }
        }

        Ok(())
    }
}

#[cfg(feature = "repl")]
#[allow(clippy::unnecessary_wraps)]
fn repl(objects: Vec<Value>, timezone: TimeZone, vrl_runtime: VrlRuntime) -> Result<(), Error> {
    use core::TargetValue;

    let objects = objects
        .into_iter()
        .map(|value| TargetValue {
            value,
            metadata: Value::Object(BTreeMap::new()),
            secrets: Secrets::new(),
        })
        .collect();

    repl::run(objects, timezone, vrl_runtime).map_err(Into::into)
}

#[cfg(not(feature = "repl"))]
#[allow(clippy::needless_pass_by_value)]
fn repl(_objects: Vec<Value>, _timezone: TimeZone, _vrl_runtime: VrlRuntime) -> Result<(), Error> {
    Err(Error::ReplFeature)
}

fn execute(
    object: &mut impl Target,
    program: &Program,
    timezone: TimeZone,
    mut runtime: Runtime,
    vrl_runtime: VrlRuntime,
) -> Result<Value, Error> {
    match vrl_runtime {
        VrlRuntime::Ast => runtime
            .resolve(object, program, &timezone)
            .map_err(Error::Runtime),
    }
}

fn serde_to_vrl(value: serde_json::Value) -> Value {
    use serde_json::Value as JsonValue;

    match value {
        JsonValue::Null => ::value::Value::Null,
        JsonValue::Object(v) => v
            .into_iter()
            .map(|(k, v)| (k, serde_to_vrl(v)))
            .collect::<BTreeMap<_, _>>()
            .into(),
        JsonValue::Bool(v) => v.into(),
        JsonValue::Number(v) if v.is_f64() => Value::from_f64_or_zero(v.as_f64().unwrap()),
        JsonValue::Number(v) => v.as_i64().unwrap_or(i64::MAX).into(),
        JsonValue::String(v) => v.into(),
        JsonValue::Array(v) => v.into_iter().map(serde_to_vrl).collect::<Vec<_>>().into(),
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
