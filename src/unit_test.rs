use crate::topology::{config::Config, unit_test::UnitTest};
use colored::*;
use std::{fs::File, path::PathBuf};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// Any number of Vector config files to test.
    paths: Vec<PathBuf>,
}

fn build_tests(path: &PathBuf) -> Result<Vec<UnitTest>, Vec<String>> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(error) => {
            if let std::io::ErrorKind::NotFound = error.kind() {
                return Err(vec![format!(
                    "Config file not found in path '{}'",
                    path.to_str().unwrap_or("")
                )]);
            } else {
                return Err(vec![format!(
                    "Could not open file '{}': {}",
                    path.to_str().unwrap_or(""),
                    error
                )]);
            }
        }
    };

    let config = match Config::load(file) {
        Err(load_errs) => {
            return Err(load_errs);
        }
        Ok(c) => c,
    };

    crate::topology::unit_test::build_unit_tests(&config)
}

pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let mut failed_files: Vec<(String, Vec<(String, Vec<String>)>)> = Vec::new();

    for (i, p) in opts.paths.iter().enumerate() {
        let path_str = p.to_str().unwrap_or("");
        if i > 0 {
            println!("");
        }
        println!("Running {} tests", path_str);
        match build_tests(p) {
            Ok(mut tests) => {
                let mut aggregated_test_errors = Vec::new();
                tests.iter_mut().for_each(|t| {
                    let test_errors = t.run();
                    if !test_errors.is_empty() {
                        println!("Test {}: {} ... {}", path_str, t.name, "failed".red());
                        aggregated_test_errors.push((t.name.clone(), test_errors));
                    } else {
                        println!("Test {}: {} ... {}", path_str, t.name, "passed".green());
                    }
                });
                if !aggregated_test_errors.is_empty() {
                    failed_files.push((path_str.to_owned(), aggregated_test_errors));
                }
            }
            Err(errs) => {
                error!("Failed to execute {} tests:\n{}", path_str, errs.join("\n"));
                return exitcode::CONFIG;
            }
        }
    }

    if !failed_files.is_empty() {
        println!("\nfailures:");
        for (path, failures) in failed_files {
            println!("\n--- {} ---", path);
            for (test_name, fails) in failures {
                println!("\nTest '{}':", test_name);
                for fail in fails {
                    println!("{}", fail);
                }
            }
        }
        exitcode::CONFIG
    } else {
        exitcode::OK
    }
}
