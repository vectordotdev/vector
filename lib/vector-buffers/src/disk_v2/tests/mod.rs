use std::{future::Future, io, path::Path, str::FromStr, sync::Arc};

use bytes::{Buf, BufMut};
use once_cell::sync::Lazy;
use temp_dir::TempDir;
use tracing_fluent_assertions::{AssertionRegistry, AssertionsLayer};
use tracing_subscriber::{filter::LevelFilter, layer::SubscriberExt, Layer, Registry};
use vector_common::byte_size_of::ByteSizeOf;

use super::Ledger;
use crate::{
    buffer_usage_data::BufferUsageHandle,
    disk_v2::{Buffer, DiskBufferConfig, Reader, Writer},
    encoding::FixedEncodable,
    Acker, Bufferable,
};

mod acknowledgements;
mod basic;
mod invariants;
mod known_errors;
mod record;
mod size_limits;

/*
    Helper code for getting tracing data from a test:

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL)
        .with_test_writer()
        .init();
*/

#[macro_export]
macro_rules! assert_buffer_is_empty {
    ($ledger:expr) => {
        assert_eq!(
            $ledger.get_total_records(),
            0,
            "ledger should have 0 records, but had {}",
            $ledger.get_total_records()
        );
        assert_eq!(
            $ledger.get_total_buffer_size(),
            0,
            "ledger should have 0 bytes, but had {} bytes",
            $ledger.get_total_buffer_size()
        );
    };
}

#[macro_export]
macro_rules! assert_buffer_records {
    ($ledger:expr, $record_count:expr) => {
        assert_eq!(
            $ledger.get_total_records(),
            $record_count as u64,
            "ledger should have {} records, but had {}",
            $record_count,
            $ledger.get_total_records()
        );
    };
}

#[macro_export]
macro_rules! assert_buffer_size {
    ($ledger:expr, $record_count:expr, $buffer_size:expr) => {
        assert_eq!(
            $ledger.get_total_records(),
            $record_count as u64,
            "ledger should have {} records, but had {}",
            $record_count,
            $ledger.get_total_records()
        );
        assert_eq!(
            $ledger.get_total_buffer_size(),
            $buffer_size as u64,
            "ledger should have {} bytes, but had {} bytes",
            $buffer_size,
            $ledger.get_total_buffer_size()
        );
    };
}

#[macro_export]
macro_rules! assert_reader_writer_file_positions {
    ($ledger:expr, $reader:expr, $writer:expr) => {{
        let (reader, writer) = $ledger.get_current_reader_writer_file_id();
        assert_eq!(
            reader,
            ($reader) as u16,
            "expected reader file ID of {}, got {} instead",
            ($reader),
            reader
        );
        assert_eq!(
            writer,
            ($writer) as u16,
            "expected writer file ID of {}, got {} instead",
            ($writer),
            writer
        );
    }};
}

#[macro_export]
macro_rules! assert_enough_bytes_written {
    ($written:expr, $record_type:ty, $record_payload_size:expr) => {
        assert!(
            $written >= $record_payload_size as usize + 8 + std::mem::size_of::<$record_type>()
        );
    };
}

#[macro_export]
macro_rules! assert_file_does_not_exist_async {
    ($file_path:expr) => {{
        let result = tokio::fs::metadata($file_path).await;
        assert!(result.is_err());
        assert_eq!(
            std::io::ErrorKind::NotFound,
            result.expect_err("is_err() was true").kind(),
            "got unexpected error kind"
        );
    }};
}

#[macro_export]
macro_rules! assert_file_exists_async {
    ($file_path:expr) => {{
        let result = tokio::fs::metadata($file_path).await;
        assert!(result.is_ok());
        assert!(
            result.expect("is_ok() was true").is_file(),
            "path exists but is not file"
        );
    }};
}

#[macro_export]
macro_rules! await_timeout {
    ($fut:expr, $secs:expr) => {{
        tokio::time::timeout(std::time::Duration::from_secs($secs), $fut)
            .await
            .expect("future should not timeout")
    }};
}

#[macro_export]
macro_rules! set_data_file_length {
    ($path:expr, $start_len:expr, $target_len:expr) => {{
        let mut data_file = OpenOptions::new()
            .write(true)
            .open(&$path)
            .await
            .expect("open should not fail");

        // Just to make sure the data file matches our expected state before futzing with it.
        let metadata = data_file
            .metadata()
            .await
            .expect("metadata should not fail");
        assert_eq!(($start_len) as u64, metadata.len());

        data_file
            .set_len(($target_len) as u64)
            .await
            .expect("truncate should not fail");
        data_file.flush().await.expect("flush should not fail");
        data_file.sync_all().await.expect("sync should not fail");
        drop(data_file);
    }};
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SizedRecord(pub u32);

impl ByteSizeOf for SizedRecord {
    fn allocated_bytes(&self) -> usize {
        self.0 as usize
    }
}

impl FixedEncodable for SizedRecord {
    type EncodeError = io::Error;
    type DecodeError = io::Error;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::EncodeError>
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

    fn decode<B>(mut buffer: B) -> Result<SizedRecord, Self::DecodeError>
    where
        B: Buf,
    {
        let buf_len = buffer.get_u32();
        buffer.advance(buf_len as usize);
        Ok(SizedRecord(buf_len))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct UndecodableRecord;

impl ByteSizeOf for UndecodableRecord {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

impl FixedEncodable for UndecodableRecord {
    type EncodeError = io::Error;
    type DecodeError = io::Error;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::EncodeError>
    where
        B: BufMut,
    {
        if buffer.remaining_mut() < 4 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "not enough capacity to encode record",
            ));
        }

        buffer.put_u32(42);
        Ok(())
    }

    fn decode<B>(_buffer: B) -> Result<UndecodableRecord, Self::DecodeError>
    where
        B: Buf,
    {
        Err(io::Error::new(io::ErrorKind::Other, "failed to decode"))
    }
}

pub(crate) async fn create_default_buffer<P, R>(
    data_dir: P,
) -> (Writer<R>, Reader<R>, Acker, Arc<Ledger>)
where
    P: AsRef<Path>,
    R: Bufferable,
{
    let config = DiskBufferConfig::from_path(data_dir).build();
    let usage_handle = BufferUsageHandle::noop();
    Buffer::from_config_inner(config, usage_handle)
        .await
        .expect("should not fail to create buffer")
}

pub(crate) async fn create_buffer_with_max_buffer_size<P, R>(
    data_dir: P,
    max_buffer_size: u64,
) -> (Writer<R>, Reader<R>, Acker, Arc<Ledger>)
where
    P: AsRef<Path>,
    R: Bufferable,
{
    // We override `max_buffer_size` directly because otherwise `build` has built-in logic that
    // ensures it is a minimum size related to the data file size limit, etc.
    let mut config = DiskBufferConfig::from_path(data_dir).build();
    config.max_buffer_size = max_buffer_size;
    let usage_handle = BufferUsageHandle::noop();

    Buffer::from_config_inner(config, usage_handle)
        .await
        .expect("should not fail to create buffer")
}

pub(crate) async fn create_buffer_with_max_record_size<P, R>(
    data_dir: P,
    max_record_size: usize,
) -> (Writer<R>, Reader<R>, Acker, Arc<Ledger>)
where
    P: AsRef<Path>,
    R: Bufferable,
{
    let config = DiskBufferConfig::from_path(data_dir)
        .max_record_size(max_record_size)
        .build();
    let usage_handle = BufferUsageHandle::noop();

    Buffer::from_config_inner(config, usage_handle)
        .await
        .expect("should not fail to create buffer")
}

pub(crate) async fn create_buffer_with_max_data_file_size<P, R>(
    data_dir: P,
    max_data_file_size: u64,
) -> (Writer<R>, Reader<R>, Acker, Arc<Ledger>)
where
    P: AsRef<Path>,
    R: Bufferable,
{
    let config = DiskBufferConfig::from_path(data_dir)
        .max_data_file_size(max_data_file_size)
        .build();
    let usage_handle = BufferUsageHandle::noop();

    Buffer::from_config_inner(config, usage_handle)
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

pub fn install_tracing_helpers() -> AssertionRegistry {
    // TODO: This installs the assertions layer globally, so all tests will run through it.  Since
    // most of the code being tested overlaps, individual tests should wrap their async code blocks
    // with a unique span that can be matched on specifically with
    // `AssertionBuilder::with_parent_name`.
    //
    // TODO: We also need a better way of wrapping our test functions in their own parent spans, for
    // the purpose of isolating their assertions.  Right now, we do it with a unique string that we
    // have set to the test function name, but this is susceptible to being copypasta'd
    // unintentionally, thus letting assertions bleed into other tests.
    //
    // Maybe we should add a helper method to `tracing-fluent-assertions` for generating a
    // uniquely-named span that can be passed directly to the assertion builder methods, then it's a
    // much tighter loop.
    //
    // TODO: At some point, we might be able to write a simple derive macro that does this for us, and
    // configures the other necessary bits, but for now.... by hand will get the job done.
    static ASSERTION_REGISTRY: Lazy<AssertionRegistry> = Lazy::new(|| {
        let assertion_registry = AssertionRegistry::default();
        let assertions_layer = AssertionsLayer::new(&assertion_registry);

        // Constrain the actual output layer to the normal RUST_LOG-based control mechanism, so that
        // assertions can run unfettered but without also spamming the console with logs.
        let fmt_filter = std::env::var("RUST_LOG")
            .map_err(|_| ())
            .and_then(|s| LevelFilter::from_str(s.as_str()).map_err(|_| ()))
            .unwrap_or(LevelFilter::OFF);
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_ansi(true)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL)
            .with_test_writer()
            .with_filter(fmt_filter);

        let base_subscriber = Registry::default();
        let subscriber = base_subscriber.with(assertions_layer).with(fmt_layer);

        tracing::subscriber::set_global_default(subscriber).unwrap();
        assertion_registry
    });

    ASSERTION_REGISTRY.clone()
}
