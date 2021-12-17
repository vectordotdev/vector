use std::path::PathBuf;

use structopt::StructOpt;

use crate::config;

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// Read configuration from one or more files. Wildcard paths are supported.
    /// File format is detected from the file name.
    /// If zero files are specified the default config path
    /// `/etc/vector/vector.toml` will be targeted.
    #[structopt(
        name = "config",
        short,
        long,
        env = "VECTOR_CONFIG",
        use_delimiter(true)
    )]
    paths: Vec<PathBuf>,

    /// Vector config files in TOML format.
    #[structopt(name = "config-toml", long, use_delimiter(true))]
    paths_toml: Vec<PathBuf>,

    /// Vector config files in JSON format.
    #[structopt(name = "config-json", long, use_delimiter(true))]
    paths_json: Vec<PathBuf>,

    /// Vector config files in YAML format.
    #[structopt(name = "config-yaml", long, use_delimiter(true))]
    paths_yaml: Vec<PathBuf>,

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

pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let paths = opts.paths_with_formats();
    let paths = match config::process_paths(&paths) {
        Some(paths) => paths,
        None => return exitcode::CONFIG,
    };

    let config = match config::load_from_paths(&paths) {
        Ok(config) => config,
        Err(errs) => {
            #[allow(clippy::print_stderr)]
            for err in errs {
                eprintln!("{}", err);
            }
            return exitcode::CONFIG;
        }
    };

    let mut dot = String::from("digraph {\n");

    for (id, _source) in &config.sources {
        dot += &format!("  \"{}\" [shape=trapezium]\n", id);
    }

    for (id, transform) in &config.transforms {
        dot += &format!("  \"{}\" [shape=diamond]\n", id);

        for input in transform.inputs.iter() {
            if let Some(port) = &input.port {
                dot += &format!(
                    "  \"{}\" -> \"{}\" [label=\"{}\"]\n",
                    input.component, id, port
                );
            } else {
                dot += &format!("  \"{}\" -> \"{}\"\n", input, id);
            }
        }
    }

    for (id, sink) in &config.sinks {
        dot += &format!("  \"{}\" [shape=invtrapezium]\n", id);

        for input in &sink.inputs {
            if let Some(port) = &input.port {
                dot += &format!(
                    "  \"{}\" -> \"{}\" [label=\"{}\"]\n",
                    input.component, id, port
                );
            } else {
                dot += &format!("  \"{}\" -> \"{}\"\n", input, id);
            }
        }
    }

    dot += "}";

    #[allow(clippy::print_stdout)]
    {
        println!("{}", dot);
    }

    exitcode::OK
}
