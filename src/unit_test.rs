#![allow(missing_docs)]
use std::path::PathBuf;

use clap::Parser;
use colored::*;

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
                for test in tests {
                    let name = test.name.clone();
                    let UnitTestResult { errors } = test.run().await;
                    if !errors.is_empty() {
                        #[allow(clippy::print_stdout)]
                        {
                            println!("test {} ... {}", name, "failed".red());
                        }
                        aggregated_test_errors.push((name, errors));
                    } else {
                        #[allow(clippy::print_stdout)]
                        {
                            println!("test {} ... {}", name, "passed".green());
                        }
                    }
                }
            }
        }
        Err(errors) => {
            error!("Failed to execute tests:\n{}.", errors.join("\n"));
            return exitcode::CONFIG;
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
