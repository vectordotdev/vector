#![allow(clippy::print_stdout)]

use std::{collections::HashMap, io::Write, path::Path, str::FromStr, time::Duration};

use value::Value;
use vector_common::TimeZone;
use vector_core::event::{Event, LogEvent, TargetEvents, VrlTarget};
use vrl::{
    compile_with_state,
    llvm::OptimizationLevel,
    state::{ExternalEnv, Runtime},
    BatchContext, Context, Target, VrlRuntime,
};

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    let mut source = String::new();
    print!("program: ");
    stdout.flush().unwrap();
    while stdin.read_line(&mut source).unwrap() > 0 {}
    let mut external_env = ExternalEnv::default();
    let functions = vrl_stdlib::all();
    let (mut program, _) = compile_with_state(&source, &functions, &mut external_env).unwrap();
    let mut local_env = program.local_env().clone();

    let runtime = {
        let mut runtime = String::new();
        print!("runtime: ");
        stdout.flush().unwrap();
        stdin.read_line(&mut runtime).unwrap();
        VrlRuntime::from_str(runtime.trim()).unwrap()
    };

    let llvm_library = if runtime == VrlRuntime::Llvm {
        let optimization_level = {
            let mut optimization_level = String::new();
            print!("optimization_level: ");
            stdout.flush().unwrap();
            stdin.read_line(&mut optimization_level).unwrap();
            match optimization_level.trim() {
                "0" | "none" => OptimizationLevel::None,
                "1" | "less" => OptimizationLevel::Less,
                "2" | "default" => OptimizationLevel::Default,
                "3" | "aggressive" => OptimizationLevel::Aggressive,
                _ => OptimizationLevel::Aggressive,
            }
        };

        let llvm_builder = vrl::llvm::Compiler::new().unwrap();
        let llvm_library = llvm_builder
            .compile(
                optimization_level,
                (&mut local_env, &mut external_env),
                &program,
                &functions,
                HashMap::new(),
            )
            .unwrap();
        llvm_library.get_function().unwrap();
        Some(llvm_library)
    } else {
        None
    };

    loop {
        let event = {
            print!("target: ");
            stdout.flush().unwrap();
            let mut target = String::new();
            while stdin.read_line(&mut target).unwrap() > 0 {}
            Event::from(
                LogEvent::try_from(serde_json::from_str::<serde_json::Value>(&target).unwrap())
                    .unwrap(),
            )
        };

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
        let timezone = TimeZone::default();

        let mut targets = Vec::new();
        let mut samples_warmup = vec![0u64; num_iterations_warmup];
        let mut samples = vec![0u64; num_iterations];

        let mut results_path = String::new();

        match runtime {
            VrlRuntime::Ast => {
                let mut run_vrl = |samples: &mut [_], i| {
                    let events = vec![event.clone(); batch_size];

                    targets = events
                        .into_iter()
                        .map(|event| VrlTarget::new(event, program.info()))
                        .collect::<Vec<_>>();

                    let start = std::time::Instant::now();

                    resolved_values.truncate(0);

                    for target in &mut targets {
                        let mut state = vrl::state::Runtime::default();
                        let mut context = Context::new(target, &mut state, &timezone);
                        let result = program.resolve(&mut context);
                        resolved_values.push(result);
                    }

                    samples[i] = std::time::Instant::now()
                        .duration_since(start)
                        .as_nanos()
                        .try_into()
                        .unwrap();
                };

                println!("Warming up...");

                for i in 0..num_iterations_warmup {
                    run_vrl(&mut samples_warmup, i);
                }

                print!("Press enter to begin.");
                stdout.flush().unwrap();
                stdin.read_line(&mut String::new()).unwrap();

                for i in 0..num_iterations {
                    run_vrl(&mut samples, i);
                }

                print!("Write results to path (enter for stdout): ");
                stdout.flush().unwrap();
                stdin.read_line(&mut results_path).unwrap();
            }
            VrlRuntime::Vectorized => {
                let mut states = Vec::<Runtime>::new();
                let mut selection_vector = Vec::new();
                let mut run_vrl = |samples: &mut [_], i| {
                    let events = vec![event.clone(); batch_size];

                    targets = events
                        .into_iter()
                        .map(|event| VrlTarget::new(event, program.info()))
                        .collect::<Vec<_>>();

                    let start = std::time::Instant::now();
                    let mut batch_targets = targets
                        .iter_mut()
                        .map(|target| target as &mut dyn Target)
                        .collect::<Vec<_>>();

                    resolved_values.resize(batch_targets.len(), Ok(Value::Null));

                    selection_vector.resize(batch_targets.len(), 0);
                    for (i, entry) in selection_vector.iter_mut().enumerate() {
                        *entry = i;
                    }

                    for state in states.iter_mut() {
                        state.clear();
                    }
                    states.resize_with(batch_targets.len(), Runtime::default);

                    let mut ctx = BatchContext::new(
                        &mut resolved_values,
                        &mut batch_targets,
                        &mut states,
                        timezone,
                    );
                    program.resolve_batch(&mut ctx, &selection_vector);

                    samples[i] = std::time::Instant::now()
                        .duration_since(start)
                        .as_nanos()
                        .try_into()
                        .unwrap();
                };

                println!("Warming up...");

                for i in 0..num_iterations_warmup {
                    run_vrl(&mut samples_warmup, i);
                }

                print!("Press enter to begin.");
                stdout.flush().unwrap();
                stdin.read_line(&mut String::new()).unwrap();

                for i in 0..num_iterations {
                    run_vrl(&mut samples, i);
                }

                print!("Write results to path (enter for stdout): ");
                stdout.flush().unwrap();
                stdin.read_line(&mut results_path).unwrap();
            }
            VrlRuntime::Llvm => {
                let vrl_execute = llvm_library.as_ref().unwrap().get_function().unwrap();

                let mut run_vrl = |samples: &mut [_], i| {
                    let events = vec![event.clone(); batch_size];

                    targets = events
                        .into_iter()
                        .map(|event| VrlTarget::new(event, program.info()))
                        .collect::<Vec<_>>();

                    let start = std::time::Instant::now();

                    resolved_values.truncate(0);

                    for target in &mut targets {
                        let mut context = vrl::core::Context {
                            target,
                            timezone: &timezone,
                        };
                        let mut result = Ok(Value::Null);
                        unsafe { vrl_execute.call(&mut context, &mut result) };
                        resolved_values.push(result);
                    }

                    samples[i] = std::time::Instant::now()
                        .duration_since(start)
                        .as_nanos()
                        .try_into()
                        .unwrap();
                };

                println!("Warming up...");

                for i in 0..num_iterations_warmup {
                    run_vrl(&mut samples_warmup, i);
                }

                print!("Press enter to begin.");
                stdout.flush().unwrap();
                stdin.read_line(&mut String::new()).unwrap();

                for i in 0..num_iterations {
                    run_vrl(&mut samples, i);
                }

                print!("Write results to path (enter for stdout): ");
                stdout.flush().unwrap();
                stdin.read_line(&mut results_path).unwrap();
            }
        }

        let samples_unsorted = samples.clone();
        samples.sort_unstable();
        let p0 = samples[0];
        let p25 = {
            let sample = samples.len() / 4;
            samples[sample]
        };
        let p50 = {
            let sample = samples.len() / 2;
            samples[sample]
        };
        let p75 = {
            let sample = samples.len() * 3 / 4;
            samples[sample]
        };
        let p100 = samples[samples.len() - 1];
        let mean = (samples.iter().sum::<u64>() as f64 / samples.len() as f64) as u64;

        let results_path = results_path.trim();
        if results_path.is_empty() {
            println!("result: {:?}", resolved_values[0]);
            println!(
                "target: {}",
                match targets[0].clone().into_events() {
                    TargetEvents::One(event) => format!("{:?}", event.into_log().into_parts().0),
                    TargetEvents::Logs(events) => format!(
                        "{:?}",
                        events
                            .map(|event| event.into_log().into_parts().0)
                            .collect::<Vec<_>>()
                    ),
                    TargetEvents::Traces(traces) => format!("{:?}", traces.collect::<Vec<_>>()),
                }
            );
            println!(
                "p0: {:?} / {:?} (per item)",
                Duration::from_nanos(p0),
                Duration::from_nanos(p0 / batch_size as u64)
            );
            println!(
                "p25: {:?} / {:?} (per item)",
                Duration::from_nanos(p25),
                Duration::from_nanos(p25 / batch_size as u64)
            );
            println!(
                "p50: {:?} / {:?} (per item)",
                Duration::from_nanos(p50),
                Duration::from_nanos(p50 / batch_size as u64)
            );
            println!(
                "p75: {:?} / {:?} (per item)",
                Duration::from_nanos(p75),
                Duration::from_nanos(p75 / batch_size as u64)
            );
            println!(
                "p100: {:?} / {:?} (per item)",
                Duration::from_nanos(p100),
                Duration::from_nanos(p100 / batch_size as u64)
            );
            println!(
                "mean: {:?} / {:?} (per item)",
                Duration::from_nanos(mean),
                Duration::from_nanos(mean / batch_size as u64)
            );
        } else {
            let results = serde_json::json!({
                "program": source.trim(),
                "runtime": runtime,
                "result": format!("{:?}", resolved_values[0]),
                "target": match targets[0].clone().into_events() {
                    TargetEvents::One(event) => format!("{:?}", event.into_log().into_parts().0),
                    TargetEvents::Logs(events) => format!(
                        "{:?}",
                        events
                            .map(|event| event.into_log().into_parts().0)
                            .collect::<Vec<_>>()
                    ),
                    TargetEvents::Traces(traces) => format!("{:?}", traces.collect::<Vec<_>>()),
                },
                "batch_size:": batch_size,
                "num_iterations_warmup:": num_iterations_warmup,
                "num_iterations:": num_iterations,
                "time": {
                    "p0": p0,
                    "p25": p25,
                    "p50": p50,
                    "p75": p75,
                    "p100": p100,
                    "mean": mean,
                },
                "time_per_item": {
                    "p0": p0 / batch_size as u64,
                    "p25": p25 / batch_size as u64,
                    "p50": p50 / batch_size as u64,
                    "p75": p75 / batch_size as u64,
                    "p100": p100 / batch_size as u64,
                    "mean": mean / batch_size as u64,
                },
                "samples": samples,
                "samples_unsorted": samples_unsorted,
            });

            let results_path = Path::new(results_path.trim());
            let mut results_file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(results_path)
                .unwrap();

            results_file
                .write_all(serde_json::to_string_pretty(&results).unwrap().as_bytes())
                .unwrap();
        }
    }
}
