use std::path::PathBuf;

use clap::Parser;
use vector_buffers::{
    disk_v2::{DiskBufferConfigBuilder, Ledger},
    BufferUsageHandle,
};

#[derive(Parser, Debug)]
#[clap(rename_all = "kebab-case")]
pub struct Opts {
    /// Actually bump the read record ID by one. Without this option, this program just outputs the
    /// current ledger state.
    #[clap(long)]
    doit: bool,
    /// Directory in which the disk buffer resides.
    data_dir: PathBuf,
}

#[allow(clippy::print_stdout, clippy::print_stderr)]
pub(crate) async fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let config = match DiskBufferConfigBuilder::from_path(&opts.data_dir).build() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Could not build disk buffer config: {}", error);
            return exitcode::CONFIG;
        }
    };

    let ledger = match Ledger::load(config, BufferUsageHandle::noop(), true).await {
        Ok(ledger) => ledger,
        Err(error) => {
            eprintln!("Could not open disk buffer ledger: {}", error);
            return exitcode::CONFIG;
        }
    };
    let state = ledger.state();

    println!(
        "Next writer record ID: {}",
        state.get_next_writer_record_id()
    );
    println!(
        "Last reader record ID: {}",
        state.get_last_reader_record_id()
    );

    if opts.doit {
        state.increment_last_reader_record_id(1);
        println!(
            "Last reader record ID advanced to {}",
            state.get_last_reader_record_id()
        );
    }

    exitcode::OK
}
