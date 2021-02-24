use crate::config;
use colored::*;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// Vector config files in TOML format to test.
    #[structopt(name = "config-toml", long)]
    paths_toml: Vec<PathBuf>,

    /// Vector config files in JSON format to test.
    #[structopt(name = "config-json", long)]
    paths_json: Vec<PathBuf>,

    /// Vector config files in YAML format to test.
    #[structopt(name = "config-yaml", long)]
    paths_yaml: Vec<PathBuf>,

    /// Any number of Vector config files to test. If none are specified the
    /// default config path `/etc/vector/vector.toml` will be targeted.
    paths: Vec<PathBuf>,
}

impl Opts {
    fn paths_with_formats(&self) -> Vec<(PathBuf, config::FormatHint)> {
        config::merge_path_lists(vec![
            (&self.paths, None),
            (&self.paths_toml, Some(config::Format::TOML)),
            (&self.paths_json, Some(config::Format::JSON)),
            (&self.paths_yaml, Some(config::Format::YAML)),
        ])
    }
}

pub async fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let mut aggregated_test_inspections = Vec::new();
    let mut aggregated_test_errors = Vec::new();

    let paths = opts.paths_with_formats();
    let paths = match config::process_paths(&paths) {
        Some(paths) => paths,
        None => return exitcode::CONFIG,
    };

    println!("Running tests");
    match config::build_unit_tests(&paths).await {
        Ok(mut tests) => {
            tests.iter_mut().for_each(|t| {
                let (test_inspections, test_errors) = t.run();
                if !test_inspections.is_empty() {
                    aggregated_test_inspections.push((t.name.clone(), test_inspections));
                }
                if !test_errors.is_empty() {
                    println!("test {} ... {}", t.name, "failed".red());
                    aggregated_test_errors.push((t.name.clone(), test_errors));
                } else {
                    println!("test {} ... {}", t.name, "passed".green());
                }
            });
            if tests.is_empty() {
                println!("{}", "No tests found.".yellow());
            }
        }
        Err(errs) => {
            error!("Failed to execute tests:\n{}.", errs.join("\n"));
            return exitcode::CONFIG;
        }
    }

    if !aggregated_test_inspections.is_empty() {
        println!("\ninspections:");
        for (test_name, inspection) in aggregated_test_inspections {
            println!("\ntest {}:\n", test_name);
            for inspect in inspection {
                println!("{}\n", inspect);
            }
        }
    }

    if !aggregated_test_errors.is_empty() {
        println!("\nfailures:");
        for (test_name, fails) in aggregated_test_errors {
            println!("\ntest {}:\n", test_name);
            for fail in fails {
                println!("{}\n", fail);
            }
        }

        exitcode::CONFIG
    } else {
        exitcode::OK
    }
}
