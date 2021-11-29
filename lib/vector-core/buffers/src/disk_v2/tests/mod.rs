use std::io;
use std::path::Path;
use std::sync::Arc;

use bytes::{Buf, BufMut};
use core_common::byte_size_of::ByteSizeOf;
use std::future::Future;
use temp_dir::TempDir;

use crate::bytes::{DecodeBytes, EncodeBytes};
use crate::disk_v2::{Buffer, DiskBufferConfig, Reader, Writer};
use crate::Bufferable;

use super::Ledger;

mod basic;
mod invariants;
mod size_limits;

/*
    Helper code for getting tracing data from a test:

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL)
        .with_test_writer()
        .init();
*/

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SizedRecord(pub u32);

impl ByteSizeOf for SizedRecord {
    fn allocated_bytes(&self) -> usize {
        self.0 as usize
    }
}

impl EncodeBytes<SizedRecord> for SizedRecord {
    type Error = io::Error;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::Error>
    where
        B: BufMut,
    {
        if buffer.remaining_mut() < self.0 as usize + 4 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "not enough capacity to encode record",
            ));
        }

        buffer.put_u32(self.0);
        buffer.put_bytes(0x42, self.0 as usize);
        Ok(())
    }
}

impl DecodeBytes<SizedRecord> for SizedRecord {
    type Error = io::Error;

    fn decode<B>(mut buffer: B) -> Result<SizedRecord, Self::Error>
    where
        B: Buf,
    {
        let buf_len = buffer.get_u32();
        let _ = buffer.advance(buf_len as usize);
        Ok(SizedRecord(buf_len))
    }
}

pub(crate) async fn create_default_buffer<P, R>(data_dir: P) -> (Writer<R>, Reader<R>, Arc<Ledger>)
where
    P: AsRef<Path>,
    R: Bufferable,
{
    Buffer::from_config_inner(DiskBufferConfig::from_path(data_dir).build())
        .await
        .expect("should not fail to create buffer")
}

pub(crate) async fn create_buffer_with_max_buffer_size<P, R>(
    data_dir: P,
    max_buffer_size: u64,
) -> (Writer<R>, Reader<R>, Arc<Ledger>)
where
    P: AsRef<Path>,
    R: Bufferable,
{
    // We override `max_buffer_size` directly because otherwise `build` has built-in logic that
    // ensures it is a minimum size related to the data file size limit, etc.
    let mut config = DiskBufferConfig::from_path(data_dir).build();
    config.max_buffer_size = max_buffer_size;

    Buffer::from_config_inner(config)
        .await
        .expect("should not fail to create buffer")
}

pub(crate) async fn create_buffer_with_max_record_size<P, R>(
    data_dir: P,
    max_record_size: usize,
) -> (Writer<R>, Reader<R>, Arc<Ledger>)
where
    P: AsRef<Path>,
    R: Bufferable,
{
    let config = DiskBufferConfig::from_path(data_dir)
        .max_record_size(max_record_size)
        .build();

    Buffer::from_config_inner(config)
        .await
        .expect("should not fail to create buffer")
}

pub(crate) async fn create_buffer_with_max_data_file_size<P, R>(
    data_dir: P,
    max_data_file_size: u64,
) -> (Writer<R>, Reader<R>, Arc<Ledger>)
where
    P: AsRef<Path>,
    R: Bufferable,
{
    let config = DiskBufferConfig::from_path(data_dir)
        .max_data_file_size(max_data_file_size)
        .build();

    Buffer::from_config_inner(config)
        .await
        .expect("should not fail to create buffer")
}

pub(crate) async fn with_temp_dir<F, Fut, V>(f: F) -> V
where
    F: FnOnce(&Path) -> Fut,
    Fut: Future<Output = V>,
{
    let buf_dir = TempDir::new().expect("creating temp dir should never fail");
    f(buf_dir.path()).await
}
