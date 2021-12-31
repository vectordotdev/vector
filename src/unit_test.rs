use std::path::PathBuf;

use colored::*;
use structopt::StructOpt;

use crate::config;

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// Vector config files in TOML format to test.
    #[structopt(name = "config-toml", long, use_delimiter(true))]
    paths_toml: Vec<PathBuf>,

    /// Vector config files in JSON format to test.
    #[structopt(name = "config-json", long, use_delimiter(true))]
    paths_json: Vec<PathBuf>,

    /// Vector config files in YAML format to test.
    #[structopt(name = "config-yaml", long, use_delimiter(true))]
    paths_yaml: Vec<PathBuf>,

    /// Any number of Vector config files to test. If none are specified the
    /// default config path `/etc/vector/vector.toml` will be targeted.
    #[structopt(use_delimiter(true))]
    paths: Vec<PathBuf>,

    /// Read configuration from files in one or more directories.
    /// File format is detected from the file name.
    ///
    /// Files not ending in .toml, .json, .yaml, or .yml will be ignored.
    #[structopt(
        name = "config-dir",
        short = "C",
        long,
        env = "VECTOR_CONFIG_DIR",
        use_delimiter(true)
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

pub async fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let mut aggregated_test_inspections = Vec::new();
    let mut aggregated_test_errors = Vec::new();

    let paths = opts.paths_with_formats();
    let paths = match config::process_paths(&paths) {
        Some(paths) => paths,
        None => return exitcode::CONFIG,
    };

    #[allow(clippy::print_stdout)]
    {
        println!("Running tests");
    }
    match config::build_unit_tests(&paths).await {
        Ok(mut tests) => {
            tests.iter_mut().for_each(|t| {
                let (test_inspections, test_errors) = t.run();
                if !test_inspections.is_empty() {
                    aggregated_test_inspections.push((t.name.clone(), test_inspections));
                }
                if !test_errors.is_empty() {
                    #[allow(clippy::print_stdout)]
                    {
                        println!("test {} ... {}", t.name, "failed".red());
                    }
                    aggregated_test_errors.push((t.name.clone(), test_errors));
                } else {
                    #[allow(clippy::print_stdout)]
                    {
                        println!("test {} ... {}", t.name, "passed".green());
                    }
                }
            });
            if tests.is_empty() {
                #[allow(clippy::print_stdout)]
                {
                    println!("{}", "No tests found.".yellow());
                }
            }
        }
        Err(errs) => {
            error!("Failed to execute tests:\n{}.", errs.join("\n"));
            return exitcode::CONFIG;
        }
    }

    if !aggregated_test_inspections.is_empty() {
        #[allow(clippy::print_stdout)]
        {
            println!("\ninspections:");
        }
        for (test_name, inspection) in aggregated_test_inspections {
            #[allow(clippy::print_stdout)]
            {
                println!("\ntest {}:\n", test_name);
            }
            for inspect in inspection {
                #[allow(clippy::print_stdout)]
                {
                    println!("{}\n", inspect);
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
