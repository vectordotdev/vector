use ansi_term::Colour;
use chrono::{DateTime, Utc};
use glob::glob;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use structopt::StructOpt;
use vrl::{diagnostic::Formatter, function::Example, state, Runtime, Value};

#[derive(Debug, StructOpt)]
#[structopt(name = "VRL Tests", about = "Vector Remap Language Tests")]
pub struct Cmd {
    #[structopt(short, long)]
    pattern: Option<String>,

    #[structopt(short, long)]
    fail_early: bool,

    #[structopt(short, long)]
    verbose: bool,

    #[structopt(long)]
    skip_functions: bool,
}

fn main() {
    let cmd = Cmd::from_args();

    let mut failed_count = 0;
    let mut category = "".to_owned();

    let tests = glob("tests/**/*.vrl")
        .expect("valid pattern")
        .into_iter()
        .filter_map(|entry| {
            let path = entry.ok()?;

            if &path.to_string_lossy() == "tests/example.vrl" {
                return None;
            }

            if let Some(pat) = &cmd.pattern {
                if !path.to_string_lossy().contains(pat) {
                    return None;
                }
            }

            Some(Test::from_path(&path))
        })
        .chain({
            let mut tests = vec![];
            stdlib::all().into_iter().for_each(|function| {
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
        .collect::<Vec<_>>();

    for mut test in tests {
        if category != test.category {
            category = test.category;
            println!("{}", Colour::Fixed(3).bold().paint(category.to_string()));
        }

        if let Some(err) = test.error {
            println!("{}", Colour::Purple.bold().paint("INVALID"));
            println!("{}", Colour::Red.paint(err));
            failed_count += 1;
            continue;
        }

        let dots = 60 - test.name.len();
        print!(
            "  {}{}",
            test.name,
            Colour::Fixed(240).paint(".".repeat(dots))
        );

        if test.skip {
            println!("{}", Colour::Yellow.bold().paint("SKIPPED"));
        }

        let state = state::Runtime::default();
        let mut runtime = Runtime::new(state);
        let program = vrl::compile(&test.source, &stdlib::all());

        let want = test.result;

        match program {
            Ok(program) => {
                let result = runtime.resolve(&mut test.object, &program);

                match result {
                    Ok(got) => {
                        if !test.skip {
                            let want = if want.starts_with("r'") && want.ends_with('\'') {
                                match regex::Regex::new(
                                    &want[2..want.len() - 1].replace("\\'", "'"),
                                ) {
                                    Ok(want) => want.into(),
                                    Err(_) => want.into(),
                                }
                            } else if want.starts_with("t'") && want.ends_with('\'') {
                                match DateTime::<Utc>::from_str(&want[2..want.len() - 1]) {
                                    Ok(want) => want.into(),
                                    Err(_) => want.into(),
                                }
                            } else if want.starts_with("s'") && want.ends_with('\'') {
                                want[2..want.len() - 1].into()
                            } else {
                                match serde_json::from_str::<'_, Value>(&want.trim()) {
                                    Ok(want) => want,
                                    Err(_) => want.into(),
                                }
                            };

                            if got == want {
                                println!("{}", Colour::Green.bold().paint("OK"));
                            } else {
                                println!("{} (expectation)", Colour::Red.bold().paint("FAILED"));
                                failed_count += 1;

                                let want = want.to_string();
                                let got = got.to_string();

                                let diff = prettydiff::diff_chars(&want, &got)
                                    .set_highlight_whitespace(true);
                                println!("  {}", diff);

                                if cmd.fail_early {
                                    std::process::exit(1)
                                }
                            }
                        }

                        if cmd.verbose {
                            println!("{:#}", got);
                        }
                    }
                    Err(err) => {
                        if !test.skip {
                            let got = err.to_string().trim().to_owned();
                            let want = want.trim().to_owned();

                            if (test.result_approx && compare_partial_diagnostic(&got, &want))
                                || got == want
                            {
                                println!("{}", Colour::Green.bold().paint("OK"));
                            } else {
                                println!("{} (runtime)", Colour::Red.bold().paint("FAILED"));
                                failed_count += 1;

                                let diff = prettydiff::diff_lines(&want, &got);
                                println!("{}", diff);

                                if cmd.fail_early {
                                    std::process::exit(1)
                                }
                            }
                        }

                        if cmd.verbose {
                            println!("{:#}", err);
                        }
                    }
                }
            }
            Err(diagnostics) => {
                let mut formatter = Formatter::new(&test.source, diagnostics);
                if !test.skip {
                    let got = formatter.to_string().trim().to_owned();
                    let want = want.trim().to_owned();

                    if (test.result_approx && compare_partial_diagnostic(&got, &want))
                        || got == want
                    {
                        println!("{}", Colour::Green.bold().paint("OK"));
                    } else {
                        println!("{} (compilation)", Colour::Red.bold().paint("FAILED"));
                        failed_count += 1;

                        let diff = prettydiff::diff_lines(&want, &got);
                        println!("{}", diff);

                        if cmd.fail_early {
                            std::process::exit(1)
                        }
                    }
                }

                if cmd.verbose {
                    formatter.enable_colors(true);
                    println!("{:#}", formatter);
                }
            }
        }
    }

    print_result(failed_count)
}

#[derive(Debug)]
struct Test {
    name: String,
    category: String,
    error: Option<String>,
    source: String,
    object: Value,
    result: String,
    result_approx: bool,
    skip: bool,
}

enum CaptureMode {
    Result,
    Object,
    None,
    Done,
}

impl Test {
    fn from_path(path: &Path) -> Self {
        let name = test_name(path);
        let category = test_category(&path);
        let content = fs::read_to_string(path).expect("content");

        let mut source = String::new();
        let mut object = String::new();
        let mut result = String::new();
        let mut result_approx = false;
        let mut skip = false;

        if content.starts_with("# SKIP") {
            skip = true;
        }

        let mut capture_mode = CaptureMode::None;
        for mut line in content.lines() {
            if line.starts_with('#') && !matches!(capture_mode, CaptureMode::Done) {
                line = line.strip_prefix('#').expect("prefix");
                line = line.strip_prefix(' ').unwrap_or(line);

                if line.starts_with("object:") {
                    capture_mode = CaptureMode::Object;
                    line = line.strip_prefix("object:").expect("object").trim_start();
                } else if line.starts_with("result: ~") {
                    capture_mode = CaptureMode::Result;
                    result_approx = true;
                    line = line.strip_prefix("result: ~").expect("result").trim_start();
                } else if line.starts_with("result:") {
                    capture_mode = CaptureMode::Result;
                    line = line.strip_prefix("result:").expect("result").trim_start();
                }

                match capture_mode {
                    CaptureMode::None | CaptureMode::Done => continue,
                    CaptureMode::Result => {
                        result.push_str(line);
                        result.push('\n');
                    }
                    CaptureMode::Object => {
                        object.push_str(line);
                    }
                }
            } else {
                capture_mode = CaptureMode::Done;

                source.push_str(line);
                source.push('\n')
            }
        }

        let mut error = None;
        let object = if object.is_empty() {
            Value::Object(BTreeMap::default())
        } else {
            match serde_json::from_str::<'_, Value>(&object) {
                Ok(value) => value,
                Err(err) => {
                    error = Some(format!("unable to parse object as JSON: {}", err));
                    Value::Null
                }
            }
        };

        result = result.trim_end().to_owned();

        Self {
            name,
            category,
            error,
            source,
            object,
            result,
            result_approx,
            skip,
        }
    }

    fn from_example(func: &'static str, example: &Example) -> Self {
        let object = Value::Object(BTreeMap::default());
        let result = match example.result {
            Ok(string) => string.to_owned(),
            Err(err) => err.to_string(),
        };

        Self {
            name: example.title.to_owned(),
            category: format!("functions/{}", func),
            error: None,
            source: example.source.to_owned(),
            object,
            result,
            result_approx: false,
            skip: false,
        }
    }
}

fn test_category(path: &Path) -> String {
    path.to_string_lossy()
        .strip_prefix("tests/")
        .expect("test")
        .rsplitn(2, '/')
        .nth(1)
        .unwrap()
        .to_owned()
}

fn test_name(path: &Path) -> String {
    path.to_string_lossy()
        .rsplitn(2, '/')
        .next()
        .unwrap()
        .trim_end_matches(".vrl")
        .replace("_", " ")
}

fn compare_partial_diagnostic(got: &str, want: &str) -> bool {
    got.lines()
        .filter(|line| line.trim().starts_with("error[E"))
        .zip(want.trim().lines())
        .all(|(got, want)| got.contains(want))
}

fn print_result(failed_count: usize) {
    let code = if failed_count > 0 { 1 } else { 0 };

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
