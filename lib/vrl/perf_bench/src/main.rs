#![allow(clippy::print_stdout)]

use serde::Serialize;
use std::{collections::HashMap, io::Write, path::Path, str::FromStr};
use value::Value;
use vector_common::TimeZone;
use vector_core::event::{Event, LogEvent, TargetEvents, VrlTarget};
use vrl::{
    compile_with_state,
    llvm::OptimizationLevel,
    state::{ExternalEnv, Runtime},
    BatchContext, Context, Target, VrlRuntime,
};

#[derive(Debug, Copy, Clone, Default, Serialize)]
struct Sample {
    pub nanos: u64,
    #[cfg(feature = "performance_counters")]
    pub cycles: u64,
    #[cfg(feature = "performance_counters")]
    pub load_store_instructions: u64,
    #[cfg(feature = "performance_counters")]
    pub l1_data_load_cache_misses: u64,
    #[cfg(feature = "performance_counters")]
    pub l1_data_store_cache_misses: u64,
}

impl Sample {
    #[cfg(feature = "performance_counters")]
    pub fn contains_empty_performance_counters(&self) -> bool {
        self.cycles == 0
            || self.load_store_instructions == 0
            || self.l1_data_load_cache_misses == 0
            || self.l1_data_store_cache_misses == 0
    }
}

impl std::ops::Div<usize> for Sample {
    type Output = Sample;

    fn div(self, rhs: usize) -> Self::Output {
        Self {
            nanos: self.nanos / rhs as u64,
            #[cfg(feature = "performance_counters")]
            cycles: self.cycles / rhs as u64,
            #[cfg(feature = "performance_counters")]
            load_store_instructions: self.load_store_instructions / rhs as u64,
            #[cfg(feature = "performance_counters")]
            l1_data_load_cache_misses: self.l1_data_load_cache_misses / rhs as u64,
            #[cfg(feature = "performance_counters")]
            l1_data_store_cache_misses: self.l1_data_store_cache_misses / rhs as u64,
        }
    }
}

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
        let mut samples = vec![Sample::default(); num_iterations];

        let mut results_path = String::new();

        match runtime {
            VrlRuntime::Ast => {
                let mut run_vrl = || {
                    let events = vec![event.clone(); batch_size];

                    targets = events
                        .into_iter()
                        .map(|event| VrlTarget::new(event, program.info()))
                        .collect::<Vec<_>>();

                    #[cfg(feature = "performance_counters")]
                    let counters_library = performance_counters::Library::new().unwrap();
                    #[cfg(feature = "performance_counters")]
                    let counting = counters_library.start_counting().unwrap();
                    #[cfg(feature = "performance_counters")]
                    let start_counters = counting.get_counters();
                    let start_time = std::time::Instant::now();

                    resolved_values.truncate(0);

                    for target in &mut targets {
                        let mut state = vrl::state::Runtime::default();
                        let mut context = Context::new(target, &mut state, &timezone);
                        let result = program.resolve(&mut context);
                        resolved_values.push(result);
                    }

                    let time = std::time::Instant::now()
                        .duration_since(start_time)
                        .as_nanos()
                        .try_into()
                        .unwrap();
                    #[cfg(feature = "performance_counters")]
                    let counters = counting.get_counters() - start_counters;

                    Sample {
                        nanos: time,
                        #[cfg(feature = "performance_counters")]
                        cycles: counters.cycles,
                        #[cfg(feature = "performance_counters")]
                        load_store_instructions: counters.load_store_instructions,
                        #[cfg(feature = "performance_counters")]
                        l1_data_load_cache_misses: counters.l1_data_load_cache_misses,
                        #[cfg(feature = "performance_counters")]
                        l1_data_store_cache_misses: counters.l1_data_store_cache_misses,
                    }
                };

                println!("Warming up...");

                #[allow(clippy::never_loop)]
                for sample in samples.iter_mut().take(num_iterations) {
                    *sample = loop {
                        let sample = run_vrl();
                        #[cfg(feature = "performance_counters")]
                        if sample.contains_empty_performance_counters() {
                            continue;
                        }
                        break sample;
                    };
                }

                samples.truncate(0);
                samples.resize(num_iterations, Sample::default());

                print!("Press enter to begin.");
                stdout.flush().unwrap();
                stdin.read_line(&mut String::new()).unwrap();

                #[allow(clippy::never_loop)]
                for sample in samples.iter_mut().take(num_iterations) {
                    *sample = loop {
                        let sample = run_vrl();
                        #[cfg(feature = "performance_counters")]
                        if sample.contains_empty_performance_counters() {
                            continue;
                        }
                        break sample;
                    };
                }

                print!("Write results to path (enter for stdout): ");
                stdout.flush().unwrap();
                stdin.read_line(&mut results_path).unwrap();
            }
            VrlRuntime::Vectorized => {
                let mut states = Vec::<Runtime>::new();
                let mut selection_vector = Vec::new();
                let mut run_vrl = || {
                    let events = vec![event.clone(); batch_size];

                    targets = events
                        .into_iter()
                        .map(|event| VrlTarget::new(event, program.info()))
                        .collect::<Vec<_>>();

                    #[cfg(feature = "performance_counters")]
                    let counters_library = performance_counters::Library::new().unwrap();
                    #[cfg(feature = "performance_counters")]
                    let counting = counters_library.start_counting().unwrap();
                    #[cfg(feature = "performance_counters")]
                    let start_counters = counting.get_counters();
                    let start_time = std::time::Instant::now();

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

                    let time = std::time::Instant::now()
                        .duration_since(start_time)
                        .as_nanos()
                        .try_into()
                        .unwrap();

                    #[cfg(feature = "performance_counters")]
                    let counters = counting.get_counters() - start_counters;

                    Sample {
                        nanos: time,
                        #[cfg(feature = "performance_counters")]
                        cycles: counters.cycles,
                        #[cfg(feature = "performance_counters")]
                        load_store_instructions: counters.load_store_instructions,
                        #[cfg(feature = "performance_counters")]
                        l1_data_load_cache_misses: counters.l1_data_load_cache_misses,
                        #[cfg(feature = "performance_counters")]
                        l1_data_store_cache_misses: counters.l1_data_store_cache_misses,
                    }
                };

                println!("Warming up...");

                #[allow(clippy::never_loop)]
                for sample in samples.iter_mut().take(num_iterations) {
                    *sample = loop {
                        let sample = run_vrl();
                        #[cfg(feature = "performance_counters")]
                        if sample.contains_empty_performance_counters() {
                            continue;
                        }
                        break sample;
                    };
                }

                samples.truncate(0);
                samples.resize(num_iterations, Sample::default());

                print!("Press enter to begin.");
                stdout.flush().unwrap();
                stdin.read_line(&mut String::new()).unwrap();

                #[allow(clippy::never_loop)]
                for sample in samples.iter_mut().take(num_iterations) {
                    *sample = loop {
                        let sample = run_vrl();
                        #[cfg(feature = "performance_counters")]
                        if sample.contains_empty_performance_counters() {
                            continue;
                        }
                        break sample;
                    };
                }

                print!("Write results to path (enter for stdout): ");
                stdout.flush().unwrap();
                stdin.read_line(&mut results_path).unwrap();
            }
            VrlRuntime::Llvm => {
                let vrl_execute = llvm_library.as_ref().unwrap().get_function().unwrap();

                let mut run_vrl = || {
                    let events = vec![event.clone(); batch_size];

                    targets = events
                        .into_iter()
                        .map(|event| VrlTarget::new(event, program.info()))
                        .collect::<Vec<_>>();

                    #[cfg(feature = "performance_counters")]
                    let counters_library = performance_counters::Library::new().unwrap();
                    #[cfg(feature = "performance_counters")]
                    let counting = counters_library.start_counting().unwrap();
                    #[cfg(feature = "performance_counters")]
                    let start_counters = counting.get_counters();
                    let start_time = std::time::Instant::now();

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

                    let time = std::time::Instant::now()
                        .duration_since(start_time)
                        .as_nanos()
                        .try_into()
                        .unwrap();

                    #[cfg(feature = "performance_counters")]
                    let counters = counting.get_counters() - start_counters;

                    Sample {
                        nanos: time,
                        #[cfg(feature = "performance_counters")]
                        cycles: counters.cycles,
                        #[cfg(feature = "performance_counters")]
                        load_store_instructions: counters.load_store_instructions,
                        #[cfg(feature = "performance_counters")]
                        l1_data_load_cache_misses: counters.l1_data_load_cache_misses,
                        #[cfg(feature = "performance_counters")]
                        l1_data_store_cache_misses: counters.l1_data_store_cache_misses,
                    }
                };

                println!("Warming up...");

                #[allow(clippy::never_loop)]
                for sample in samples.iter_mut().take(num_iterations) {
                    *sample = loop {
                        let sample = run_vrl();
                        #[cfg(feature = "performance_counters")]
                        if sample.contains_empty_performance_counters() {
                            continue;
                        }
                        break sample;
                    };
                }

                samples.truncate(0);
                samples.resize(num_iterations, Sample::default());

                print!("Press enter to begin.");
                stdout.flush().unwrap();
                stdin.read_line(&mut String::new()).unwrap();

                #[allow(clippy::never_loop)]
                for sample in samples.iter_mut().take(num_iterations) {
                    *sample = loop {
                        let sample = run_vrl();
                        #[cfg(feature = "performance_counters")]
                        if sample.contains_empty_performance_counters() {
                            continue;
                        }
                        break sample;
                    };
                }

                print!("Write results to path (enter for stdout): ");
                stdout.flush().unwrap();
                stdin.read_line(&mut results_path).unwrap();
            }
        }

        let samples_unsorted = samples.clone();
        samples.sort_unstable_by(|a, b| a.nanos.cmp(&b.nanos));

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
            println!("p0: {:?}", p0);
            println!("p25: {:?}", p25);
            println!("p50: {:?}", p50);
            println!("p75: {:?}", p75);
            println!("p100: {:?}", p100);
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
                "p0": p0,
                "p0_per_item": p0 / batch_size,
                "p25": p25,
                "p25_per_item": p25 / batch_size,
                "p50": p50,
                "p50_per_item": p50 / batch_size,
                "p75": p75,
                "p75_per_item": p75 / batch_size,
                "p100": p100,
                "p100_per_item": p100 / batch_size,
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
