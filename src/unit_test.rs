#![allow(missing_docs)]
use std::fs::File;
use std::io::prelude::*;
use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use colored::*;
use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestSuite};

use crate::config::{self, UnitTestResult};
use crate::signal;

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
pub struct Opts {
    /// Vector config files in TOML format to test.
    #[arg(id = "config-toml", long, value_delimiter(','))]
    paths_toml: Vec<PathBuf>,

    /// Vector config files in JSON format to test.
    #[arg(id = "config-json", long, value_delimiter(','))]
    paths_json: Vec<PathBuf>,

    /// Vector config files in YAML format to test.
    #[arg(id = "config-yaml", long, value_delimiter(','))]
    paths_yaml: Vec<PathBuf>,

    /// Any number of Vector config files to test. If none are specified the
    /// default config path `/etc/vector/vector.yaml` will be targeted.
    #[arg(value_delimiter(','))]
    paths: Vec<PathBuf>,

    /// Read configuration from files in one or more directories.
    /// File format is detected from the file name.
    ///
    /// Files not ending in .toml, .json, .yaml, or .yml will be ignored.
    #[arg(
        id = "config-dir",
        short = 'C',
        long,
        env = "VECTOR_CONFIG_DIR",
        value_delimiter(',')
    )]
    pub config_dirs: Vec<PathBuf>,

    /// Output path for JUnit reports
    #[arg(id = "junit-report", long, value_delimiter(','))]
    junit_report_paths: Option<Vec<PathBuf>>,
}

impl Opts {
    fn paths_with_formats(&self) -> Vec<config::ConfigPath> {
        config::merge_path_lists(vec![
            (&self.paths, None),
            (&self.paths_toml, Some(config::Format::Toml)),
            (&self.paths_json, Some(config::Format::Json)),
            (&self.paths_yaml, Some(config::Format::Yaml)),
        ])
        .map(|(path, hint)| config::ConfigPath::File(path, hint))
        .chain(
            self.config_dirs
                .iter()
                .map(|dir| config::ConfigPath::Dir(dir.to_path_buf())),
        )
        .collect()
    }
}

pub async fn cmd(opts: &Opts, signal_handler: &mut signal::SignalHandler) -> exitcode::ExitCode {
    let mut aggregated_test_errors: Vec<(String, Vec<String>)> = Vec::new();

    let paths = opts.paths_with_formats();
    let paths = match config::process_paths(&paths) {
        Some(paths) => paths,
        None => return exitcode::CONFIG,
    };

    let create_junit_report = opts.path_junit_report.is_some();
    let mut report = Report::new("Vector Unit Tests");
    let mut test_suite = TestSuite::new("Test Suite");

    #[allow(clippy::print_stdout)]
    {
        println!("Running tests");
    }
    match config::build_unit_tests_main(&paths, signal_handler).await {
        Ok(tests) => {
            if tests.is_empty() {
                #[allow(clippy::print_stdout)]
                {
                    println!("{}", "No tests found.".yellow());
                }
            } else {
                let test_suite_start = Instant::now();
                for test in tests {
                    let name = test.name.clone();

                    let test_case_start = Instant::now();
                    let UnitTestResult { errors } = test.run().await;
                    let test_case_elapsed = test_case_start.elapsed();

                    if !errors.is_empty() {
                        #[allow(clippy::print_stdout)]
                        {
                            println!("test {} ... {}", name, "failed".red());
                        }

                        if create_junit_report {
                            // create a failed test case
                            let errors_joined = errors.join("\n");
                            let mut status = TestCaseStatus::non_success(NonSuccessKind::Failure);
                            status.set_description(errors_joined);
                            let mut test_case = TestCase::new(name.clone(), status);
                            test_case.set_time(test_case_elapsed);
                            test_suite.add_test_case(test_case);
                        }

                        aggregated_test_errors.push((name, errors));
                    } else {
                        #[allow(clippy::print_stdout)]
                        {
                            println!("test {} ... {}", name, "passed".green());
                        }

                        if create_junit_report {
                            // create a successful test case
                            let mut test_case = TestCase::new(name, TestCaseStatus::success());
                            test_case.set_time(test_case_elapsed);
                            test_suite.add_test_case(test_case);
                        }
                    }
                }

                if create_junit_report {
                    // create a report from the test cases
                    let test_suite_elapsed = test_suite_start.elapsed();
                    test_suite.set_time(test_suite_elapsed);
                    report.add_test_suite(test_suite);
                }
            }
        }
        Err(errors) => {
            error!("Failed to execute tests:\n{}.", errors.join("\n"));
            return exitcode::CONFIG;
        }
    }

    if create_junit_report {
        // write junit report
        let report_bytes = match report.to_string() {
            Ok(report_string) => report_string.into_bytes(),
            Err(error) => {
                error!("Failed to execute tests:\n{}.", error);
                return exitcode::CONFIG;
            }
        };

        for path in &opts.path_junit_report.unwrap() { // safe to unwrap because `create_junit_report` is true
            match File::create(path) {
                Ok(mut file) => {
                    if let Err(error) = file.write_all(&report_bytes) {
                        error!("Failed to execute tests:\n{}.", error);
                        return exitcode::CONFIG;
                    }
                }
                Err(error) => {
                    error!("Failed to execute tests:\n{}.", error);
                    return exitcode::CONFIG;
                }
            }
        }
    }

    if !aggregated_test_errors.is_empty() {
        #[allow(clippy::print_stdout)]
        {
            println!("\nfailures:");
        }
        for (test_name, fails) in aggregated_test_errors {
            #[allow(clippy::print_stdout)]
            {
                println!("\ntest {}:\n", test_name);
            }
            for fail in fails {
                #[allow(clippy::print_stdout)]
                {
                    println!("{}\n", fail);
                }
            }
        }

        exitcode::CONFIG
    } else {
        exitcode::OK
    }
}
