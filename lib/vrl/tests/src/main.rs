#![allow(clippy::print_stdout)] // tests
#![allow(clippy::print_stderr)] // tests

mod test_enrichment;

use std::{str::FromStr, time::Instant};

use ::value::Value;
use ansi_term::Colour;
use chrono::{DateTime, SecondsFormat, Utc};
use chrono_tz::Tz;
use clap::Parser;
use glob::glob;
use value::Secrets;
use vector_common::TimeZone;
use vrl::{
    diagnostic::Formatter,
    prelude::{BTreeMap, VrlValueConvert},
    state, CompilationResult, CompileConfig, Runtime, SecretTarget, TargetValueRef, Terminate,
    VrlRuntime,
};
use vrl_tests::{docs, Test};

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Parser, Debug)]
#[clap(name = "VRL Tests", about = "Vector Remap Language Tests")]
pub struct Cmd {
    #[clap(short, long)]
    pattern: Option<String>,

    #[clap(short, long)]
    fail_early: bool,

    #[clap(short, long)]
    verbose: bool,

    #[clap(short, long)]
    no_diff: bool,

    /// When enabled, any log output at the INFO or above level is printed
    /// during the test run.
    #[clap(short, long)]
    logging: bool,

    /// When enabled, show run duration for each individual test.
    #[clap(short, long)]
    timings: bool,

    #[clap(short = 'z', long)]
    timezone: Option<String>,

    /// Should we use the VM to evaluate the VRL
    #[clap(short, long = "runtime", default_value_t)]
    runtime: VrlRuntime,

    /// Ignore the Cue tests (to speed up run)
    #[clap(long)]
    ignore_cue: bool,
}

impl Cmd {
    fn timezone(&self) -> TimeZone {
        if let Some(ref tz) = self.timezone {
            TimeZone::parse(tz).unwrap_or_else(|| panic!("couldn't parse timezone: {}", tz))
        } else {
            TimeZone::Named(Tz::UTC)
        }
    }
}

fn should_run(name: &str, pat: &Option<String>, _runtime: VrlRuntime) -> bool {
    if name == "tests/example.vrl" {
        return false;
    }

    if let Some(pat) = pat {
        if !name.contains(pat) {
            return false;
        }
    }

    true
}

fn main() {
    let cmd = Cmd::parse();

    if cmd.logging {
        tracing_subscriber::fmt::init();
    }

    let mut failed_count = 0;
    let mut category = "".to_owned();

    let tests = glob("tests/**/*.vrl")
        .expect("valid pattern")
        .into_iter()
        .filter_map(|entry| {
            let path = entry.ok()?;
            Some(Test::from_path(&path))
        })
        .chain({
            let mut tests = vec![];
            stdlib::all()
                .into_iter()
                .chain(enrichment::vrl_functions())
                .for_each(|function| {
                    if let Some(closure) = function.closure() {
                        closure.inputs.iter().for_each(|input| {
                            let test = Test::from_example(
                                format!("{} (closure)", function.identifier()),
                                &input.example,
                            );

                            if let Some(pat) = &cmd.pattern {
                                if !format!("{}/{}", test.category, test.name).contains(pat) {
                                    return;
                                }
                            }

                            tests.push(test);
                        });
                    }

                    function.examples().iter().for_each(|example| {
                        let test = Test::from_example(function.identifier(), example);

                        if let Some(pat) = &cmd.pattern {
                            if !format!("{}/{}", test.category, test.name).contains(pat) {
                                return;
                            }
                        }

                        tests.push(test)
                    })
                });

            tests.into_iter()
        })
        .chain(docs::tests(cmd.ignore_cue).into_iter())
        .filter(|test| {
            should_run(
                &format!("{}/{}", test.category, test.name),
                &cmd.pattern,
                cmd.runtime,
            )
        })
        .collect::<Vec<_>>();

    for mut test in tests {
        if category != test.category {
            category = test.category.clone();
            println!("{}", Colour::Fixed(3).bold().paint(category.to_string()));
        }

        if let Some(err) = test.error {
            println!("{}", Colour::Purple.bold().paint("INVALID"));
            println!("{}", Colour::Red.paint(err));
            failed_count += 1;
            continue;
        }

        let mut name = test.name.clone();
        name.truncate(58);

        let dots = if name.len() >= 60 { 0 } else { 60 - name.len() };

        print!("  {}{}", name, Colour::Fixed(240).paint(".".repeat(dots)));

        if test.skip {
            println!("{}", Colour::Yellow.bold().paint("SKIPPED"));
        }

        let state = state::Runtime::default();
        let runtime = Runtime::new(state);
        let mut functions = stdlib::all();
        functions.append(&mut enrichment::vrl_functions());
        functions.append(&mut vector_vrl_functions::vrl_functions());
        let test_enrichment = test_enrichment::test_enrichment_table();

        let external_env = vrl::state::ExternalEnv::default();
        let mut config = CompileConfig::default();
        config.set_custom(test_enrichment.clone());

        // Set some read-only paths that can be tested
        for (path, recursive) in &test.read_only_paths {
            config.set_read_only_path(path.clone(), *recursive);
        }

        let compile_start = Instant::now();
        let result = vrl::compile_with_external(&test.source, &functions, &external_env, config);
        let compile_end = compile_start.elapsed();

        let want = test.result.clone();
        let timezone = cmd.timezone();

        let compile_timing_fmt = cmd
            .timings
            .then(|| format!("comp: {:>9.3?}", compile_end))
            .unwrap_or_default();

        match result {
            Ok(CompilationResult {
                program,
                warnings,
                config: _,
            }) if warnings.is_empty() => {
                let run_start = Instant::now();
                let result = run_vrl(
                    runtime,
                    program,
                    &mut test,
                    timezone,
                    cmd.runtime,
                    test_enrichment,
                );
                let run_end = run_start.elapsed();

                let timings_fmt = cmd
                    .timings
                    .then(|| format!(" ({}, run: {:>9.3?})", compile_timing_fmt, run_end))
                    .unwrap_or_default();

                let timings_color = if run_end.as_millis() > 10 { 1 } else { 245 };
                let timings = Colour::Fixed(timings_color).paint(timings_fmt);

                match result {
                    Ok(got) => {
                        let got = vrl_value_to_json_value(got);
                        let mut failed = false;

                        if !test.skip {
                            let want = if want.starts_with("r'") && want.ends_with('\'') {
                                match regex::Regex::new(
                                    &want[2..want.len() - 1].replace("\\'", "'"),
                                ) {
                                    Ok(want) => want.to_string().into(),
                                    Err(_) => want.into(),
                                }
                            } else if want.starts_with("t'") && want.ends_with('\'') {
                                match DateTime::<Utc>::from_str(&want[2..want.len() - 1]) {
                                    Ok(want) => {
                                        want.to_rfc3339_opts(SecondsFormat::AutoSi, true).into()
                                    }
                                    Err(_) => want.into(),
                                }
                            } else if want.starts_with("s'") && want.ends_with('\'') {
                                want[2..want.len() - 1].into()
                            } else {
                                match serde_json::from_str::<'_, serde_json::Value>(want.trim()) {
                                    Ok(want) => want,
                                    Err(err) => {
                                        eprintln!("{}", err);
                                        want.into()
                                    }
                                }
                            };
                            if got == want {
                                print!("{}{}", Colour::Green.bold().paint("OK"), timings,);
                            } else {
                                print!("{} (expectation)", Colour::Red.bold().paint("FAILED"));
                                failed_count += 1;

                                if !cmd.no_diff {
                                    let want = serde_json::to_string_pretty(&want).unwrap();
                                    let got = serde_json::to_string_pretty(&got).unwrap();

                                    let diff = prettydiff::diff_lines(&want, &got);
                                    println!("  {}", diff);
                                }

                                failed = true;
                            }

                            println!();
                        }

                        if cmd.verbose {
                            println!("{:#}", got);
                        }

                        if failed && cmd.fail_early {
                            std::process::exit(1)
                        }
                    }
                    Err(err) => {
                        let mut failed = false;
                        if !test.skip {
                            let got = err.to_string().trim().to_owned();
                            let want = want.trim().to_owned();

                            if (test.result_approx && compare_partial_diagnostic(&got, &want))
                                || got == want
                            {
                                println!("{}{}", Colour::Green.bold().paint("OK"), timings);
                            } else if matches!(err, Terminate::Abort { .. }) {
                                let want =
                                    match serde_json::from_str::<'_, serde_json::Value>(&want) {
                                        Ok(want) => want,
                                        Err(err) => {
                                            eprintln!("{}", err);
                                            want.into()
                                        }
                                    };

                                let got = vrl_value_to_json_value(test.object.clone());
                                if got == want {
                                    println!("{}{}", Colour::Green.bold().paint("OK"), timings);
                                } else {
                                    println!("{} (abort)", Colour::Red.bold().paint("FAILED"));
                                    failed_count += 1;

                                    if !cmd.no_diff {
                                        let want = serde_json::to_string_pretty(&want).unwrap();
                                        let got = serde_json::to_string_pretty(&got).unwrap();
                                        let diff = prettydiff::diff_lines(&want, &got);
                                        println!("{}", diff);
                                    }

                                    failed = true;
                                }
                            } else {
                                println!("{} (runtime)", Colour::Red.bold().paint("FAILED"));
                                failed_count += 1;

                                if !cmd.no_diff {
                                    let diff = prettydiff::diff_lines(&want, &got);
                                    println!("{}", diff);
                                }

                                failed = true;
                            }
                        }

                        if cmd.verbose {
                            println!("{:#}", err);
                        }

                        if failed && cmd.fail_early {
                            std::process::exit(1)
                        }
                    }
                }
            }
            Ok(CompilationResult {
                program: _,
                warnings: diagnostics,
                config: _,
            })
            | Err(diagnostics) => {
                let mut failed = false;
                let mut formatter = Formatter::new(&test.source, diagnostics);
                if !test.skip {
                    let got = formatter.to_string().trim().to_owned();
                    let want = want.trim().to_owned();

                    if (test.result_approx && compare_partial_diagnostic(&got, &want))
                        || got == want
                    {
                        let timings_fmt = cmd
                            .timings
                            .then(|| format!(" ({})", compile_timing_fmt))
                            .unwrap_or_default();
                        let timings = Colour::Fixed(245).paint(timings_fmt);

                        println!("{}{}", Colour::Green.bold().paint("OK"), timings);
                    } else {
                        println!("{} (compilation)", Colour::Red.bold().paint("FAILED"));
                        failed_count += 1;

                        if !cmd.no_diff {
                            let diff = prettydiff::diff_lines(&want, &got);
                            println!("{}", diff);
                        }

                        failed = true;
                    }
                }

                if cmd.verbose {
                    formatter.enable_colors(true);
                    println!("{:#}", formatter);
                }

                if failed && cmd.fail_early {
                    std::process::exit(1)
                }
            }
        }
    }

    print_result(failed_count)
}

#[allow(clippy::too_many_arguments)]
fn run_vrl(
    mut runtime: Runtime,
    program: vrl::Program,
    test: &mut Test,
    timezone: TimeZone,
    vrl_runtime: VrlRuntime,
    test_enrichment: enrichment::TableRegistry,
) -> Result<Value, Terminate> {
    let mut metadata = Value::from(BTreeMap::new());
    let mut target = TargetValueRef {
        value: &mut test.object,
        metadata: &mut metadata,
        secrets: &mut Secrets::new(),
    };

    // Insert a dummy secret for examples to use
    target.insert_secret("my_secret", "secret value");
    target.insert_secret("datadog_api_key", "secret value");

    match vrl_runtime {
        VrlRuntime::Ast => {
            test_enrichment.finish_load();
            runtime.resolve(&mut target, &program, &timezone)
        }
    }
}

fn compare_partial_diagnostic(got: &str, want: &str) -> bool {
    got.lines()
        .filter(|line| line.trim().starts_with("error[E"))
        .zip(want.trim().lines())
        .all(|(got, want)| got.contains(want))
}

fn print_result(failed_count: usize) {
    let code = i32::from(failed_count > 0);

    println!("\n");

    if failed_count > 0 {
        println!(
            "  Overall result: {}\n\n    Number failed: {}\n",
            Colour::Red.bold().paint("FAILED"),
            failed_count
        );
    } else {
        println!(
            "  Overall result: {}\n",
            Colour::Green.bold().paint("SUCCESS")
        );
    }

    std::process::exit(code)
}

fn vrl_value_to_json_value(value: Value) -> serde_json::Value {
    use serde_json::Value::*;

    match value {
        v @ Value::Bytes(_) => String(v.try_bytes_utf8_lossy().unwrap().into_owned()),
        Value::Integer(v) => v.into(),
        Value::Float(v) => v.into_inner().into(),
        Value::Boolean(v) => v.into(),
        Value::Object(v) => v
            .into_iter()
            .map(|(k, v)| (k, vrl_value_to_json_value(v)))
            .collect::<serde_json::Value>(),
        Value::Array(v) => v
            .into_iter()
            .map(vrl_value_to_json_value)
            .collect::<serde_json::Value>(),
        Value::Timestamp(v) => v.to_rfc3339_opts(SecondsFormat::AutoSi, true).into(),
        Value::Regex(v) => v.to_string().into(),
        Value::Null => Null,
    }
}
