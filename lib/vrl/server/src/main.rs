use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use structopt::StructOpt;
use vrl::{diagnostic::Formatter, state, Runtime, Value};
use warp::{Filter, Reply};

#[derive(Debug, thiserror::Error)]
enum Error {}

#[derive(Debug, StructOpt)]
struct Opts {
    #[structopt(short = "p", long, default_value = "8080", env = "PORT")]
    port: u16,
}

#[derive(Deserialize, Serialize)]
struct Input {
    program: String,
    event: Value,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Outcome {
    Success(Value, Value),
    Error(String),
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let opts = Opts::from_args();

    let resolve = warp::path("resolve")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(resolve_input);

    let _ = warp::serve(resolve)
        .run(([127, 0, 0, 1], opts.port))
        .await;

    Ok(())
}

async fn resolve_input(input: Input) -> Result<impl Reply, Infallible> {
    let outcome = compile(input);
    Ok(warp::reply::json(&outcome))
}

fn compile(mut input: Input) -> Outcome {
    let event = &mut input.event;
    let mut state = state::Compiler::default();
    let mut runtime = Runtime::new(state::Runtime::default());

    let program = match vrl::compile_with_state(&input.program, &stdlib::all(), &mut state) {
        Ok(program) => program,
        Err(diagnostics) => {
            let msg = Formatter::new(&input.program, diagnostics).to_string();
            return Outcome::Error(msg);
        }
    };

    match runtime.resolve(event, &program) {
        Ok(result) => Outcome::Success(result, event.clone()),
        Err(err) => Outcome::Error(err.to_string()),
    }
}
