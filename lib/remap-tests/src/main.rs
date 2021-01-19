use ansi_term::Colour;
use glob::glob;
use remap::{diagnostic::Formatter, prelude::*, Program, Runtime};
use std::fs;
use std::path::PathBuf;

fn main() {
    let verbose = std::env::args()
        .nth(1)
        .map(|s| s == "--verbose")
        .unwrap_or_default();

    let mut failed_count = 0;
    for entry in glob("tests/**/*.vrl").expect("valid pattern") {
        let path = match entry {
            Ok(path) => path,
            Err(_) => continue,
        };

        let mut test = Test::new(path);

        print!("{:.<70}", test.name);

        if test.skip {
            println!("{}", Colour::Yellow.bold().paint("SKIPPED"));
        }

        let state = state::Program::default();
        let mut runtime = Runtime::new(state);
        let program = Program::new(test.source.clone(), &remap_functions::all(), None, true);
        let want = test.result.to_string();

        match program {
            Ok((program, diagnostics)) => {
                let mut formatter = Formatter::new(&test.source, diagnostics);
                let result = runtime.run(&mut test.object, &program);

                match result {
                    Ok(value) => {
                        let got = value.to_string();

                        if got == want {
                            if !test.skip {
                                println!("{}", Colour::Green.bold().paint("OK"));
                            }
                        } else {
                            if !test.skip {
                                println!("{} (expectation)", Colour::Red.bold().paint("FAILED"));
                                failed_count += 1;
                            }

                            let diff =
                                prettydiff::diff_chars(&want, &got).set_highlight_whitespace(true);
                            println!("  {}", diff);
                        }

                        if verbose {
                            formatter.enable_colors(true);
                            println!("{}", formatter);
                            println!("{}", value);
                        }
                    }
                    Err(err) => {
                        let got = err.to_string().trim().to_owned();
                        let want = want.trim().to_owned();

                        if got == want {
                            if !test.skip {
                                println!("{}", Colour::Green.bold().paint("OK"));
                            }
                        } else {
                            if !test.skip {
                                println!("{} (runtime)", Colour::Red.bold().paint("FAILED"));
                                failed_count += 1;
                            }

                            let diff = prettydiff::diff_lines(&want, &got);
                            println!("{}", diff);
                        }

                        if verbose {
                            println!("{:#}", err);
                        }
                    }
                }
            }
            Err(diagnostics) => {
                let mut formatter = Formatter::new(&test.source, diagnostics);
                let got = formatter.to_string().trim().to_owned();
                let want = want.trim().to_owned();

                if got == want {
                    println!("{}", Colour::Green.bold().paint("OK"));
                } else {
                    if !test.skip {
                        println!("{} (compilation)", Colour::Red.bold().paint("FAILED"));
                        failed_count += 1;
                    }

                    let diff = prettydiff::diff_lines(&want, &got);
                    println!("{}", diff);
                }

                if verbose {
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
    skip: bool,
}

enum CaptureMode {
    Result,
    Object,
    None,
}

impl Test {
    fn new(path: PathBuf) -> Self {
        let name = path
            .to_string_lossy()
            .strip_prefix("tests/")
            .expect("test")
            .to_owned();

        let content = fs::read_to_string(path).expect("content");

        let mut source = String::new();
        let mut object = String::new();
        let mut result = String::new();
        let mut skip = false;

        if content.starts_with("# SKIP") {
            skip = true;
        }

        let mut capture_mode = CaptureMode::None;
        for mut line in content.lines() {
            if line.starts_with('#') {
                line = line.strip_prefix('#').expect("prefix");
                line = line.strip_prefix(' ').unwrap_or(line);

                if line.starts_with("object:") {
                    capture_mode = CaptureMode::Object;
                    line = line.strip_prefix("object:").expect("object").trim_start();
                } else if line.starts_with("result:") {
                    capture_mode = CaptureMode::Result;
                    line = line.strip_prefix("result:").expect("result").trim_start();
                }

                match capture_mode {
                    CaptureMode::None => continue,
                    CaptureMode::Result => {
                        result.push_str(line);
                        result.push('\n');
                    }
                    CaptureMode::Object => {
                        object.push_str(line);
                    }
                }
            } else {
                source.push_str(line);
                source.push('\n')
            }
        }

        let object = serde_json::from_str::<'_, Value>(&object).expect("valid object");

        result = result.trim_end().to_owned();

        Self {
            name,
            source,
            object,
            result,
            skip,
        }
    }
}
