use ansi_term::Colour;
use chrono::{DateTime, Utc};
use glob::glob;
use remap::{diagnostic::Formatter, state, Runtime, Value};
use std::fs;
use std::path::Path;
use std::str::FromStr;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "VRL Tests", about = "Vector Remap Language Tests")]
pub struct Cmd {
    #[structopt(short, long)]
    pattern: Option<String>,

    #[structopt(short, long)]
    verbose: bool,
}

fn main() {
    let cmd = Cmd::from_args();

    let mut failed_count = 0;
    let mut category = "".to_owned();
    for entry in glob("tests/**/*.vrl").expect("valid pattern") {
        let path = match entry {
            Ok(path) => path,
            Err(_) => continue,
        };

        if &path.to_string_lossy() == "tests/example.vrl" {
            continue;
        }

        if let Some(pat) = &cmd.pattern {
            if !path.to_string_lossy().contains(pat) {
                continue;
            }
        }

        let test_category = test_category(&path);
        if category != test_category {
            category = test_category;
            println!("{}", Colour::Fixed(3).bold().paint(format!("{}", category)));
        }

        let name = test_name(&path);
        let dots = 60 - name.len();

        print!("  {}{}", name, Colour::Fixed(240).paint(".".repeat(dots)));

        let mut test = match Test::new(&path) {
            Ok(test) => test,
            Err(err) => {
                println!("{}", Colour::Purple.bold().paint("INVALID"));
                println!("{}", Colour::Red.paint(err));
                failed_count += 1;

                continue;
            }
        };

        if test.skip {
            println!("{}", Colour::Yellow.bold().paint("SKIPPED"));
        }

        let state = state::Runtime::default();
        let mut runtime = Runtime::new(state);
        let program = remap::compile(&test.source, &remap_functions::all());

        let want = test.result;

        match program {
            Ok(program) => {
                let result = runtime.resolve(&mut test.object, &program);

                match result {
                    Ok(got) => {
                        if !test.skip {
                            let want = if want.starts_with("r'") {
                                match regex::Regex::new(
                                    &want[2..want.len() - 1].replace("\\'", "'"),
                                ) {
                                    Ok(want) => want.into(),
                                    Err(_) => want.into(),
                                }
                            } else if want.starts_with("t'") {
                                match DateTime::<Utc>::from_str(&want[2..want.len() - 1]) {
                                    Ok(want) => want.into(),
                                    Err(_) => want.into(),
                                }
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

                            if test.result_approx && compare_partial_diagnostic(&got, &want) {
                                println!("{}", Colour::Green.bold().paint("OK"));
                            } else if got == want {
                                println!("{}", Colour::Green.bold().paint("OK"));
                            } else {
                                println!("{} (runtime)", Colour::Red.bold().paint("FAILED"));
                                failed_count += 1;

                                let diff = prettydiff::diff_lines(&want, &got);
                                println!("{}", diff);
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

                    if test.result_approx && compare_partial_diagnostic(&got, &want) {
                        println!("{}", Colour::Green.bold().paint("OK"));
                    } else if got == want {
                        println!("{}", Colour::Green.bold().paint("OK"));
                    } else {
                        println!("{} (compilation)", Colour::Red.bold().paint("FAILED"));
                        failed_count += 1;

                        let diff = prettydiff::diff_lines(&want, &got);
                        println!("{}", diff);
                    }
                }

                if cmd.verbose {
                    formatter.enable_colors(true);
                    println!("{:#}", formatter);
                }
            }
        }
    }

    if failed_count > 0 {
        std::process::exit(1)
    }
}

#[derive(Debug)]
struct Test {
    name: String,
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
    fn new(path: &Path) -> Result<Self, String> {
        let name = test_name(path);
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

        let object = if object.is_empty() {
            Value::Object(std::collections::BTreeMap::default())
        } else {
            serde_json::from_str::<'_, Value>(&object)
                .map_err(|err| format!("unable to parse object as JSON: {}", err))?
        };

        result = result.trim_end().to_owned();

        Ok(Self {
            name,
            source,
            object,
            result,
            result_approx,
            skip,
        })
    }
}

fn test_category(path: &Path) -> String {
    path.to_string_lossy()
        .strip_prefix("tests/")
        .expect("test")
        .rsplitn(2, "/")
        .skip(1)
        .next()
        .unwrap()
        .to_owned()
}

fn test_name(path: &Path) -> String {
    path.to_string_lossy()
        .rsplitn(2, "/")
        .next()
        .unwrap()
        .trim_end_matches(".vrl")
        .replace("_", " ")
        .to_owned()
}

fn compare_partial_diagnostic(got: &str, want: &str) -> bool {
    let got = got
        .lines()
        .filter(|line| line.trim().starts_with("error: "))
        .collect::<Vec<_>>();

    let want = want.trim().lines().collect::<Vec<_>>();

    got == want
}
