use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
//use tracing::{span, Level};

use vector_buffers::{
    disk_v2::{DiskBufferConfigBuilder, Ledger},
    BufferUsageHandle,
};

#[derive(Debug, Parser)]
#[clap(version)]
struct Args {
    /// Actually bump the read record ID by one. Without this option, this program just outputs the
    /// current ledger state.
    #[clap(long)]
    doit: bool,
    /// Directory in which the disk buffer resides.
    data_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let config = DiskBufferConfigBuilder::from_path(args.data_dir)
        .build()
        .context("Could not build disk buffer config")?;
    let ledger = Ledger::load(config, BufferUsageHandle::noop(), true)
        .await
        .context("Could not open disk buffer ledger")?;
    let state = ledger.state();

    println!(
        "Next writer record ID: {}",
        state.get_next_writer_record_id()
    );
    println!(
        "Last reader record ID: {}",
        state.get_last_reader_record_id()
    );

    if args.doit {
        state.increment_last_reader_record_id(1);
        println!(
            "Last reader record ID advanced to {}",
            state.get_last_reader_record_id()
        );
    }

    Ok(())
}
