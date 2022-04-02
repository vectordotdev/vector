use std::path::PathBuf;

use clap::{Parser, Subcommand};
use exitcode::ExitCode;

use crate::validate;

mod disk_v1_to_disk_v2;
use disk_v1_to_disk_v2::run_disk_v1_to_disk_v2_migration;

#[derive(Subcommand, Debug)]
#[clap(rename_all = "kebab-case")]
pub enum MigrationOp {
    /// Migrate a disk v1 buffer to disk v2.
    DiskV1ToDiskV2 {
        #[clap(long)]
        sink_id: String,
    },
}

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
pub struct Opts {
    /// A Vector configuration file to load.
    ///
    /// The configuration type is detected from the file extension.  Multiple file paths can be
    /// specified by separating them with a comma, and the option itself can be passed multiple
    /// times.
    ///
    /// If none are specified, the default config path `/etc/vector/vector.toml` will be targeted.
    #[clap(
        name = "config-path",
        short = 'c',
        long,
        env = "VECTOR_CONFIG",
        use_value_delimiter(true),
        multiple_occurrences(true)
    )]
    paths: Vec<PathBuf>,

    /// A directory to read Vector configuration files from.
    ///
    /// The configuration type is detected from the file extension. Files not ending in .toml,
    /// .json, .yaml, or .yml will be ignored.
    ///
    /// Multiple directory paths can be specified by separating them with a comma, and the option
    /// itself can be passed multiple times.
    #[clap(
        name = "config-dir",
        short = 'C',
        long,
        env = "VECTOR_CONFIG_DIR",
        use_value_delimiter(true)
    )]
    pub config_dirs: Vec<PathBuf>,

    /// The migration operation to run.
    #[clap(subcommand)]
    pub migration_op: MigrationOp,
}

/// Performs migration.
pub async fn cmd(opts: &Opts, color: bool) -> ExitCode {
    info!("Validating configuration before triggering migration...");

    // We need a valid configuration before we can try and perform any migrations.  We end up
    // reusing the existing validation logic but it's slightly kludgy.  Ah well. :)
    let mut fmt = validate::Formatter::new(color);
    let validate_opts = validate::Opts {
        no_environment: true,
        deny_warnings: false,
        paths_toml: Vec::new(),
        paths_json: Vec::new(),
        paths_yaml: Vec::new(),
        paths: opts.paths.clone(),
        config_dirs: opts.config_dirs.clone(),
    };

    let config = match validate::validate_config(&validate_opts, &mut fmt) {
        Some(config) => config,
        None => {
            error!("Given configuration was not valid. Cannot proceed with migration.");
            return exitcode::CONFIG;
        }
    };

    info!("Configuration valid, triggering migration...");

    // Now attempt to run the migration.
    match &opts.migration_op {
        MigrationOp::DiskV1ToDiskV2 { sink_id } => {
            run_disk_v1_to_disk_v2_migration(&config, sink_id.as_str())
                .await
                .map_err(|e| {
                    error!("Failed to run disk-v1-to-disk-v2 migration: {}.", e);
                    exitcode::SOFTWARE
                })
        }
    }
    .map_or_else(|e| e, |()| exitcode::OK)
}
