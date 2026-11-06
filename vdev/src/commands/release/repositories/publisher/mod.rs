//! Native implementation of `vdev release repositories`.
// Repository object names are case-sensitive protocol identifiers.
#![allow(clippy::case_sensitive_file_extension_comparisons)]
mod bundles;
mod engine;
mod inputs;
mod model;
mod repository;
mod rollback;
mod storage;
use super::{Channel, Commands, Rollback};
use anyhow::Result;
use model::Config;
use std::path::Path;

pub(super) fn exec(config: &Path, work: &Path, command: Commands) -> Result<()> {
    let config = Config::load(config)?;
    let work = work.canonicalize()?;
    match command {
        Commands::Fetch { run, tag, channel } => inputs::fetch(
            &config,
            &work,
            run,
            &tag,
            match channel {
                Channel::Stable => "stable",
                Channel::Beta => "beta",
            },
        ),
        Commands::Base => engine::base(&config, &work),
        Commands::Prepare => {
            println!("Prepared snapshot {}", repository::prepare(&config, &work)?);
            Ok(())
        }
        Commands::Publish => engine::publish(&config, &work),
        Commands::Verify => {
            engine::verify_snapshot(&config, &engine::check_result(&config, &work)?.1)
        }
        Commands::Rollback { phase } => match phase {
            Rollback::Base { snapshot } => rollback::base(&config, &work, &snapshot),
            Rollback::Prepare => {
                println!(
                    "Prepared rollback snapshot {}",
                    rollback::prepare(&config, &work)?
                );
                Ok(())
            }
            Rollback::Publish => rollback::publish(&config, &work),
            Rollback::Verify => {
                engine::verify_snapshot(&config, &rollback::check_result(&config, &work)?.1)
            }
        },
        Commands::Status | Commands::RecoverLock { .. } => {
            let store = storage::S3Store::new(&config)?;
            let mut publisher = storage::Publisher::new(&store, &config);
            if let Commands::RecoverLock {
                expected_version,
                confirm_owner_stopped,
            } = command
            {
                publisher.recover_lock(&expected_version, confirm_owner_stopped)?;
                println!("Lock released; resume pending publication or roll back");
            } else {
                let (state, head) = publisher.load_state()?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({"state":state,"version":head.map(|h|h.version)})
                    )?
                );
            }
            Ok(())
        }
    }
}
