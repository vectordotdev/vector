#[cfg(feature = "repl")]
use super::repl;
use super::Error;
use shared::TimeZone;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read};
use std::iter::IntoIterator;
use std::path::PathBuf;
use stdlib::{vrl_fn_downcase, vrl_fn_string, vrl_fn_upcase};
use structopt::StructOpt;
use vrl::{diagnostic::Formatter, state, Program, Runtime, Target, Value};

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

#[cfg(test)]
impl Opts {
    fn new_test(
        program: Option<String>,
        input_file: Option<PathBuf>,
        program_file: Option<PathBuf>,
        print_object: bool,
        timezone: Option<String>,
    ) -> Self {
        Self {
            program,
            input_file,
            program_file,
            print_object,
            timezone,
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
        let mut compiler_state = vrl::state::Compiler::new();
        let program = vrl::compile_with_state(&source, &stdlib::all(), &mut compiler_state)
            .map_err(|diagnostics| {
                Error::Parse(Formatter::new(&source, diagnostics).colored().to_string())
            })?;

        for mut object in objects {
            //for _ in 0..1000000 {
            let result = execute(&compiler_state, &mut object, &program, &tz).map(|v| {
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
            //}
        }

        Ok(())
    }
}

#[cfg(test)]
#[test]
fn test_run() {
    println!("{:?}", std::env::current_dir());
    let opts = Opts::new_test(
        None,
        Some("./lib/vrl/cli/test.jsonl".into()),
        Some("./lib/vrl/cli/program.vrl".into()),
        false,
        None,
    );

    run(&opts).unwrap();
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

fn execute(
    compiler_state: &vrl::state::Compiler,
    object: &mut impl Target,
    program: &Program,
    timezone: &TimeZone,
) -> Result<Value, Error> {
    {
        // A hack to force Rust not to remove those symbols from the binary.
        // There must be a better way?
        let _ = vrl_fn_downcase(&mut Ok(Value::Null), &mut Ok(Value::Null));
        let _ = vrl_fn_upcase(&mut Ok(Value::Null), &mut Ok(Value::Null));
        let _ = vrl_fn_string(&mut Ok(Value::Null), &mut Ok(Value::Null));
    }

    {
        let state = state::Runtime::default();
        let mut runtime = Runtime::new(state);
        println!("Traverse target: {:?}", object);
        println!(
            "Traverse result: {:?}",
            runtime.resolve(object, program, timezone)
        );
        println!("Traverse target: {:?}", object);
        let start = std::time::Instant::now();
        for _ in 0..1000000 {
            runtime.clear();
            let _ = runtime.resolve(object, program, timezone);
        }
        println!("elapsed Traverse: {:?}", std::time::Instant::now() - start);
    }

    {
        let mut state = state::Runtime::default();
        let builder = vrl::llvm::Builder::new().unwrap();
        println!("VRL -> LLVM IR");
        let context = builder.compile(compiler_state, program).unwrap();
        println!("Optimize LLVM IR");
        context.optimize();
        println!("LLVM IR -> Machine Code");
        let execute = context.get_jit_function().unwrap();
        println!("LLVM target: {:?}", object);

        let mut context = vrl::Context::new(object, &mut state, timezone);

        println!("LLVM result: {:?}", {
            let mut result = Ok(Value::Null);
            unsafe { execute.call(&mut context, &mut result) };
            result
        });

        let start = std::time::Instant::now();
        for _ in 0..1000000 {
            context.state_mut().clear();
            let mut result = Ok(Value::Null);
            unsafe { execute.call(&mut context, &mut result) };
        }
        println!("elapsed LLVM: {:?}", std::time::Instant::now() - start);
    }

    {
        let state = state::Runtime::default();
        let mut runtime = Runtime::new(state);
        let mut vm = runtime.compile(stdlib::all(), program).unwrap();

        println!("VM target: {:?}", object);
        println!("VM result: {:?}", runtime.run_vm(&mut vm, object, timezone));

        let start = std::time::Instant::now();
        for _ in 0..1000000 {
            runtime.clear();
            let _ = runtime.run_vm(&mut vm, object, timezone);
        }
        println!("elapsed VM: {:?}", std::time::Instant::now() - start);
    }

    Ok(Value::Null)
    // runtime
    //    .resolve(object, program, timezone)
    //   .map_err(Error::Runtime)
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
