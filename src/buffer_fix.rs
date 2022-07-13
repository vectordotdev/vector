use std::{mem::ManuallyDrop, path::PathBuf};

use clap::Parser;
use vector_buffers::{
    disk_v2::{Buffer, DiskBufferConfigBuilder},
    BufferUsageHandle, EventCount as _,
};
use vector_common::finalization::Finalizable;
use vector_core::event::{EventArray, EventStatus};

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

    let (_writer, mut reader, ledger) = match Buffer::<EventArray>::from_config_inner(
        config,
        BufferUsageHandle::noop(),
        true,
    )
    .await
    {
        Ok(buffer) => buffer,
        Err(error) => {
            eprintln!("Could not open disk buffer: {}", error);
            return exitcode::IOERR;
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

    match reader.next().await {
        Ok(Some(record)) => {
            let mut record = ManuallyDrop::new(record);
            let count = record.event_count();
            println!("Next record size: {} events", count);

            if opts.doit {
                record
                    .take_finalizers()
                    .update_status(EventStatus::Delivered);
                drop(ManuallyDrop::into_inner(record));
                println!("Marked record as delivered");
            }
        }
        Ok(None) => println!("Buffer has no more records to read."),
        Err(error) => {
            eprintln!("Error reading next record from the buffer: {}", error);
            return exitcode::IOERR;
        }
    }

    exitcode::OK
}
