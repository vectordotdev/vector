#![allow(clippy::print_stdout)]

use std::io::Write;

use value::Value;
use vector_common::TimeZone;
use vector_core::event::{Event, LogEvent, VrlTarget};
use vrl::{compile_with_state, state::ExternalEnv, BatchContext, Context, Target};

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    let mut source = String::new();
    print!("program: ");
    stdout.flush().unwrap();
    while stdin.read_line(&mut source).unwrap() > 0 {}
    let mut state = ExternalEnv::default();
    let (mut program, _) = compile_with_state(&source, &vrl_stdlib::all(), &mut state).unwrap();

    print!("target: ");
    stdout.flush().unwrap();
    let mut target = String::new();
    stdin.read_line(&mut target).unwrap();
    let event = Event::from(
        LogEvent::try_from(serde_json::from_str::<serde_json::Value>(&target).unwrap()).unwrap(),
    );

    let num_iterations_warmup = {
        print!("num_iterations_warmup: ");
        stdout.flush().unwrap();
        let mut num_iterations_warmup = String::new();
        stdin.read_line(&mut num_iterations_warmup).unwrap();
        num_iterations_warmup.trim().parse::<usize>().unwrap()
    };
    let num_iterations = {
        print!("num_iterations: ");
        stdout.flush().unwrap();
        let mut num_iterations = String::new();
        stdin.read_line(&mut num_iterations).unwrap();
        num_iterations.trim().parse::<usize>().unwrap()
    };
    let batch_size = {
        print!("batch_size: ");
        stdout.flush().unwrap();
        let mut batch_size = String::new();
        stdin.read_line(&mut batch_size).unwrap();
        batch_size.trim().parse::<usize>().unwrap()
    };

    let mut resolved_values = vec![Ok(Value::Null); batch_size];
    let mut selection_vector = Vec::with_capacity(batch_size);
    let mut states = Vec::<vrl::state::Runtime>::with_capacity(batch_size);
    let timezone = TimeZone::default();

    let mut runtime = String::new();
    print!("runtime: ");
    stdout.flush().unwrap();
    stdin.read_line(&mut runtime).unwrap();

    let mut targets = Vec::new();
    let duration;

    match runtime.as_str().trim() {
        "ast" => {
            let mut run_vrl = || {
                let events = vec![event.clone(); batch_size];

                targets = events
                    .into_iter()
                    .map(|event| VrlTarget::new(event, program.info()))
                    .collect::<Vec<_>>();

                resolved_values.truncate(0);

                for target in &mut targets {
                    let mut state = vrl::state::Runtime::default();
                    let mut context = Context::new(target, &mut state, &timezone);
                    let result = program.resolve(&mut context);
                    resolved_values.push(result);
                }
            };

            for _ in 0..num_iterations_warmup {
                run_vrl();
            }

            print!("Press enter to begin.");
            stdout.flush().unwrap();
            stdin.read_line(&mut String::new()).unwrap();

            let start = std::time::Instant::now();

            for _ in 0..num_iterations {
                run_vrl();
            }

            duration = std::time::Instant::now().duration_since(start);

            print!("Press enter to end.");
            stdout.flush().unwrap();
            stdin.read_line(&mut String::new()).unwrap();
        }
        "ast_batched" => {
            let mut run_vrl_batched = || {
                let events = vec![event.clone(); batch_size];

                selection_vector.truncate(0);
                for i in 0..batch_size {
                    selection_vector.push(i);
                }

                targets = events
                    .into_iter()
                    .map(|event| VrlTarget::new(event, program.info()))
                    .collect::<Vec<_>>();
                let mut batch_targets = targets
                    .iter_mut()
                    .map(|target| target as &mut dyn Target)
                    .collect::<Vec<_>>();

                for state in &mut states {
                    state.clear();
                }
                states.resize_with(batch_targets.len(), vrl::state::Runtime::default);

                let mut ctx = BatchContext::new(
                    &mut resolved_values,
                    &mut batch_targets,
                    &mut states,
                    timezone,
                );
                program.resolve_batch(&mut ctx, &selection_vector);
            };

            for _ in 0..num_iterations_warmup {
                run_vrl_batched();
            }

            print!("Press enter to begin.");
            stdout.flush().unwrap();
            stdin.read_line(&mut String::new()).unwrap();

            let start = std::time::Instant::now();

            for _ in 0..num_iterations {
                run_vrl_batched();
            }

            duration = std::time::Instant::now().duration_since(start);

            print!("Press enter to end.");
            stdout.flush().unwrap();
            stdin.read_line(&mut String::new()).unwrap();
        }
        runtime => panic!("Invalid runtime {runtime}"),
    };

    println!("program: {}", source);
    println!("target: {:?}", targets[0]);
    println!("num_iterations_warmup: {}", num_iterations_warmup);
    println!("num_iterations: {}", num_iterations);
    println!("batch_size: {}", batch_size);
    println!("runtime: {}", runtime);
    println!("result: {:?}", resolved_values[0]);
    println!("duration: {:?}", duration);
}
