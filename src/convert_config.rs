use crate::config::{format, ConfigBuilder, Format};
use clap::Parser;
use colored::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Parser, Debug)]
#[command(rename_all = "kebab-case")]
pub struct Opts {
    /// The input path. It can be a single file or a directory. If this points to a directory,
    /// all files with a "toml", "yaml" or "json" extension will be converted.
    pub(crate) input_path: PathBuf,

    /// The output file or directory to be created. This command will fail if the output directory exists.
    pub(crate) output_path: PathBuf,

    /// The target format to which existing config files will be converted to.
    #[arg(long, default_value = "yaml")]
    pub(crate) output_format: Format,
}

fn check_paths(opts: &Opts) -> Result<(), String> {
    let in_metadata = fs::metadata(&opts.input_path)
        .unwrap_or_else(|_| panic!("Failed to get metadata for: {:?}", &opts.input_path));

    if opts.output_path.exists() {
        return Err(format!(
            "Output path {:?} already exists. Please provide a non-existing output path.",
            opts.output_path
        ));
    }

    if opts.output_path.extension().is_none() {
        if in_metadata.is_file() {
            return Err(format!(
                "{:?} points to a file but {:?} points to a directory.",
                opts.input_path, opts.output_path
            ));
        }
    } else if in_metadata.is_dir() {
        return Err(format!(
            "{:?} points to a directory but {:?} points to a file.",
            opts.input_path, opts.output_path
        ));
    }

    Ok(())
}

pub(crate) fn cmd(opts: &Opts) -> exitcode::ExitCode {
    if let Err(e) = check_paths(opts) {
        #[allow(clippy::print_stderr)]
        {
            eprintln!("{}", e.red());
        }
        return exitcode::SOFTWARE;
    }

    return if opts.input_path.is_file() && opts.output_path.extension().is_some() {
        if let Some(base_dir) = opts.output_path.parent() {
            if !base_dir.exists() {
                fs::create_dir_all(base_dir).unwrap_or_else(|_| {
                    panic!("Failed to create output dir(s): {:?}", &opts.output_path)
                });
            }
        }

        match convert_config(&opts.input_path, &opts.output_path, opts.output_format) {
            Ok(_) => exitcode::OK,
            Err(errors) => {
                #[allow(clippy::print_stderr)]
                {
                    errors.iter().for_each(|e| eprintln!("{}", e.red()));
                }
                exitcode::SOFTWARE
            }
        }
    } else {
        match walk_dir_and_convert(&opts.input_path, &opts.output_path, opts.output_format) {
            Ok(()) => {
                #[allow(clippy::print_stdout)]
                {
                    println!(
                        "Finished conversion(s). Results are in {:?}",
                        opts.output_path
                    );
                }
                exitcode::OK
            }
            Err(errors) => {
                #[allow(clippy::print_stderr)]
                {
                    errors.iter().for_each(|e| eprintln!("{}", e.red()));
                }
                exitcode::SOFTWARE
            }
        }
    };
}

fn convert_config(
    input_path: &Path,
    output_path: &Path,
    output_format: Format,
) -> Result<(), Vec<String>> {
    if output_path.exists() {
        return Err(vec![format!("Output path {output_path:?} exists")]);
    }
    let input_format = match Format::from_str(
        input_path
            .extension()
            .unwrap_or_else(|| panic!("Failed to get extension for: {input_path:?}"))
            .to_str()
            .unwrap_or_else(|| panic!("Failed to convert OsStr to &str for: {input_path:?}")),
    ) {
        Ok(format) => format,
        Err(_) => return Ok(()), // skip irrelevant files
    };

    if input_format == output_format {
        return Ok(());
    }

    #[allow(clippy::print_stdout)]
    {
        println!("Converting {input_path:?} config to {output_format:?}.");
    }
    let file_contents = fs::read_to_string(input_path).map_err(|e| vec![e.to_string()])?;
    let builder: ConfigBuilder = format::deserialize(&file_contents, input_format)?;
    let config = builder.build()?;
    let output_string =
        format::serialize(&config, output_format).map_err(|e| vec![e.to_string()])?;
    fs::write(output_path, output_string).map_err(|e| vec![e.to_string()])?;

    #[allow(clippy::print_stdout)]
    {
        println!("Wrote result to {output_path:?}.");
    }
    Ok(())
}

fn walk_dir_and_convert(
    input_path: &Path,
    output_dir: &Path,
    output_format: Format,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    if input_path.is_dir() {
        for entry in fs::read_dir(input_path)
            .unwrap_or_else(|_| panic!("Failed to read dir: {input_path:?}"))
        {
            let entry_path = entry
                .unwrap_or_else(|_| panic!("Failed to get entry for dir: {input_path:?}"))
                .path();
            let new_output_dir = if entry_path.is_dir() {
                let last_component = entry_path
                    .file_name()
                    .unwrap_or_else(|| panic!("Failed to get file_name for {entry_path:?}"));
                let new_dir = output_dir.join(last_component);

                if !new_dir.exists() {
                    fs::create_dir_all(&new_dir)
                        .unwrap_or_else(|_| panic!("Failed to create output dir: {new_dir:?}"));
                }
                new_dir
            } else {
                output_dir.to_path_buf()
            };

            if let Err(new_errors) = walk_dir_and_convert(
                &input_path.join(&entry_path),
                &new_output_dir,
                output_format,
            ) {
                errors.extend(new_errors);
            }
        }
    } else {
        let output_path = output_dir.join(
            input_path
                .with_extension(output_format.to_string().as_str())
                .file_name()
                .ok_or_else(|| {
                    vec![format!(
                        "Cannot create output path for input: {input_path:?}"
                    )]
                })?,
        );
        if let Err(new_errors) = convert_config(input_path, &output_path, output_format) {
            errors.extend(new_errors);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(all(
    test,
    feature = "sources-demo_logs",
    feature = "transforms-remap",
    feature = "sinks-console"
))]
mod tests {
    use crate::config::{format, ConfigBuilder, Format};
    use crate::convert_config::{check_paths, walk_dir_and_convert, Opts};
    use std::path::{Path, PathBuf};
    use std::str::FromStr;
    use std::{env, fs};
    use tempfile::tempdir;

    fn test_data_dir() -> PathBuf {
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("tests/data/cmd/config")
    }

    // Read the contents of the specified `path` and deserialize them into a `ConfigBuilder`.
    // Finally serialize them a string again. Configs do not implement equality,
    // so for these tests we will rely on strings for comparisons.
    fn convert_file_to_config_string(path: &Path) -> String {
        let files_contents = fs::read_to_string(path).unwrap();
        let extension = path.extension().unwrap().to_str().unwrap();
        let file_format = Format::from_str(extension).unwrap();
        let builder: ConfigBuilder = format::deserialize(&files_contents, file_format).unwrap();
        let config = builder.build().unwrap();

        format::serialize(&config, file_format).unwrap()
    }

    #[test]
    fn invalid_path_opts() {
        let check_error = |opts, pattern| {
            let error = check_paths(&opts).unwrap_err();
            assert!(error.contains(pattern));
        };

        check_error(
            Opts {
                input_path: ["./"].iter().collect(),
                output_path: ["./"].iter().collect(),
                output_format: Format::Yaml,
            },
            "already exists",
        );

        check_error(
            Opts {
                input_path: ["./"].iter().collect(),
                output_path: ["./out.yaml"].iter().collect(),
                output_format: Format::Yaml,
            },
            "points to a file.",
        );

        check_error(
            Opts {
                input_path: [test_data_dir(), "config_2.toml".into()].iter().collect(),
                output_path: ["./another_dir"].iter().collect(),
                output_format: Format::Yaml,
            },
            "points to a directory.",
        );
    }

    #[test]
    fn convert_all_from_dir() {
        let input_path = test_data_dir();
        let output_dir = tempdir()
            .expect("Unable to create tempdir for config")
            .into_path();
        walk_dir_and_convert(&input_path, &output_dir, Format::Yaml).unwrap();

        let mut count: usize = 0;
        let original_config = convert_file_to_config_string(&test_data_dir().join("config_1.yaml"));
        for entry in fs::read_dir(&output_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_file() {
                let extension = path.extension().unwrap().to_str().unwrap();
                if extension == Format::Yaml.to_string() {
                    // Note that here we read the converted string directly.
                    let converted_config = fs::read_to_string(output_dir.join(&path)).unwrap();
                    assert_eq!(converted_config, original_config);
                    count += 1;
                }
            }
        }
        // There two non-yaml configs in the input directory.
        assert_eq!(count, 2);
    }
}
