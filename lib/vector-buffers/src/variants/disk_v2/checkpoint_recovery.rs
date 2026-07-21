//! Startup recovery for reconciling `disk_v2` data files with the durable ledger checkpoint.
//!
//! Normal reads operate from the in-memory reader state in `reader.rs`. This module handles the
//! narrower startup path where the on-disk files may be ahead of, behind, or partially inconsistent
//! with the last flushed ledger state after a crash.

use std::{
    io::{self, ErrorKind},
    path::Path,
};

use crc32fast::Hasher;
use tracing::{debug, error, warn};

use super::{
    Filesystem,
    common::{MAX_FILE_ID, data_file_id_in_range, parse_data_file_id},
    io::AsyncFile,
    reader::{BufferReader, ReadToken, ReaderError, RecordReader, decode_record_payload},
    record::{RecordStatus, try_as_record_archive, validate_record_archive},
};
use crate::Bufferable;

struct CheckpointRecord {
    id: u64,
    bytes: u64,
    events: Option<u64>,
    next_id: Option<u64>,
    last_id: Option<u64>,
}

impl CheckpointRecord {
    /// Preserves the validated record span when its payload cannot provide event-count metadata.
    fn undecodable(id: u64, bytes: u64) -> Self {
        Self {
            id,
            bytes,
            events: None,
            next_id: None,
            last_id: None,
        }
    }

    /// Returns the last record ID covered by the reader checkpoint classification.
    fn checkpoint_last_id(&self) -> u64 {
        // Reader checkpoints advance only after an entire record is acknowledged. If the payload
        // cannot provide its event count, its starting ID is therefore enough to classify its
        // validated byte span as either acknowledged or unread.
        self.last_id.unwrap_or(self.id)
    }
}

#[derive(Default)]
struct CheckpointBoundaryScanResult {
    logical_file_size: u64,
    acknowledged_prefix_bytes: u64,
    last_checkpoint_record_id: Option<u64>,
    last_checkpoint_record_next_id: Option<u64>,
    has_trailing_corrupt_records: bool,
}

#[derive(Clone, Copy)]
struct CheckpointBoundaryFile<'a> {
    id: u16,
    path: &'a Path,
}

struct CheckpointBoundaryScan<'a> {
    file: CheckpointBoundaryFile<'a>,
    acknowledged_through_record_id: Option<u64>,
    writer_next_record_id: Option<u64>,
    result: CheckpointBoundaryScanResult,
}

enum DataFileClassification {
    Missing,
    Empty,
    KnownLastRecord { last_record_id: u64 },
    NeedsBoundaryScan,
}

impl CheckpointBoundaryScanResult {
    fn current_offset(&self) -> u64 {
        self.logical_file_size
    }

    /// Advances the logical file boundary and, when applicable, the acknowledged prefix boundary.
    fn include_record(&mut self, record: &CheckpointRecord, is_acknowledged_prefix: bool) {
        let record_end_offset = self.logical_file_size + record.bytes;
        if is_acknowledged_prefix {
            self.acknowledged_prefix_bytes = record_end_offset;
        }
        self.logical_file_size = record_end_offset;
        self.last_checkpoint_record_id = Some(record.checkpoint_last_id());
        self.last_checkpoint_record_next_id = record.next_id;
        self.has_trailing_corrupt_records = false;
    }

    /// Includes a complete corrupt record whose physical byte span is still part of the file.
    fn include_corrupt_record(&mut self, record_bytes: u64) {
        self.logical_file_size += record_bytes;
        self.has_trailing_corrupt_records = true;
    }

    /// Returns whether the validated prefix proves that the next frame starts at or beyond the
    /// durable writer checkpoint.
    fn corrupt_record_is_provably_post_checkpoint(&self, writer_next_record_id: u64) -> bool {
        writer_next_record_id == 1
            || self.last_checkpoint_record_next_id == Some(writer_next_record_id)
    }

    /// Extends the acknowledged prefix through corrupt records that the durable reader checkpoint
    /// proves were already traversed.
    fn reconcile_corrupt_acknowledged_prefix(
        &mut self,
        skip_through_record_id: u64,
        next_record_id: Option<u64>,
    ) {
        if !self.has_trailing_corrupt_records {
            return;
        }

        let checkpoint_is_after_last_valid_record = self
            .last_checkpoint_record_id
            .is_some_and(|last_record_id| last_record_id < skip_through_record_id);
        let checkpoint_is_before_next_valid_record =
            next_record_id.is_none_or(|record_id| skip_through_record_id < record_id);

        if checkpoint_is_after_last_valid_record && checkpoint_is_before_next_valid_record {
            self.acknowledged_prefix_bytes = self.logical_file_size;
        }
    }

    fn include_reader_record(&mut self, record: &CheckpointRecord, skip_through_record_id: u64) {
        self.reconcile_corrupt_acknowledged_prefix(skip_through_record_id, Some(record.id));
        self.include_record(
            record,
            record.checkpoint_last_id() <= skip_through_record_id,
        );
    }

    /// Returns the checkpoint-live bytes after excluding the acknowledged prefix.
    fn unread_bytes(mut self, skip_through_record_id: u64) -> u64 {
        self.reconcile_corrupt_acknowledged_prefix(skip_through_record_id, None);
        debug_assert!(self.logical_file_size >= self.acknowledged_prefix_bytes);
        self.logical_file_size - self.acknowledged_prefix_bytes
    }
}

impl<'a> CheckpointBoundaryScan<'a> {
    fn reader(file: CheckpointBoundaryFile<'a>, acknowledged_through_record_id: u64) -> Self {
        Self::new(file, Some(acknowledged_through_record_id), None)
    }

    fn writer(file: CheckpointBoundaryFile<'a>, writer_next_record_id: u64) -> Self {
        Self::new(file, None, Some(writer_next_record_id))
    }

    fn reader_writer(
        file: CheckpointBoundaryFile<'a>,
        acknowledged_through_record_id: u64,
        writer_next_record_id: u64,
    ) -> Self {
        Self::new(
            file,
            Some(acknowledged_through_record_id),
            Some(writer_next_record_id),
        )
    }

    fn new(
        file: CheckpointBoundaryFile<'a>,
        acknowledged_through_record_id: Option<u64>,
        writer_next_record_id: Option<u64>,
    ) -> Self {
        Self {
            file,
            acknowledged_through_record_id,
            writer_next_record_id,
            result: CheckpointBoundaryScanResult::default(),
        }
    }

    fn current_offset(&self) -> u64 {
        self.result.current_offset()
    }

    fn corrupt_record_is_provably_post_checkpoint(&self) -> bool {
        self.writer_next_record_id
            .is_some_and(|writer_next_record_id| {
                self.result
                    .corrupt_record_is_provably_post_checkpoint(writer_next_record_id)
            })
    }

    fn include_corrupt_record(&mut self, record_bytes: u64) {
        self.result.include_corrupt_record(record_bytes);
    }

    fn include_record(&mut self, record: &CheckpointRecord) {
        if let Some(acknowledged_through_record_id) = self.acknowledged_through_record_id {
            self.result
                .include_reader_record(record, acknowledged_through_record_id);
        } else {
            self.result.include_record(record, false);
        }
    }

    fn recovered_bytes(self) -> u64 {
        if let Some(acknowledged_through_record_id) = self.acknowledged_through_record_id {
            self.result.unread_bytes(acknowledged_through_record_id)
        } else {
            self.result.logical_file_size
        }
    }
}

impl<T, FS> BufferReader<T, FS>
where
    T: Bufferable,
    FS: Filesystem,
    FS::File: Unpin,
{
    /// Reconciles the checkpointed data-file window and returns its authoritative unread byte
    /// count.
    pub(super) async fn reconcile_checkpoint_window(&self) -> Result<u64, ReaderError<T>> {
        let reader_file_id = self.ledger().get_current_reader_file_id();
        let writer_file_id = self.ledger().get_current_writer_file_id();
        let reader_last_record_id = self.ledger().state().get_last_reader_record_id();
        let writer_next_record_id = self.ledger().state().get_next_writer_record_id();
        let checkpoint_data_file_ids = self
            .checkpoint_data_file_ids(reader_file_id, writer_file_id)
            .await?;
        // The ledger's reader file ID is only the durable file checkpoint. It can lag behind the
        // durable record checkpoint when the reader acknowledged records in later files but crashed
        // before flushing the advanced file ID, so re-evaluate the effective reader boundary from
        // `reader_last_record_id`.
        let effective_reader_file_id = self
            .find_checkpoint_reader_file_id(
                &checkpoint_data_file_ids,
                writer_file_id,
                reader_last_record_id,
            )
            .await?;
        let mut unread_buffer_size = 0;
        let effective_reader_position = checkpoint_data_file_ids
            .iter()
            .position(|&data_file_id| data_file_id == effective_reader_file_id)
            .expect("effective reader file ID must be in the checkpoint window");

        for &data_file_id in &checkpoint_data_file_ids[effective_reader_position..] {
            let data_file_path = self.ledger().get_data_file_path(data_file_id);
            let boundary_file = CheckpointBoundaryFile {
                id: data_file_id,
                path: &data_file_path,
            };
            unread_buffer_size += if effective_reader_file_id == writer_file_id {
                let scan = CheckpointBoundaryScan::reader_writer(
                    boundary_file,
                    reader_last_record_id,
                    writer_next_record_id,
                );
                let scan = self.reconcile_checkpoint_boundary_data_file(scan).await?;
                scan.recovered_bytes()
            } else if data_file_id == effective_reader_file_id {
                let scan = CheckpointBoundaryScan::reader(boundary_file, reader_last_record_id);
                let scan = self.reconcile_checkpoint_boundary_data_file(scan).await?;
                scan.recovered_bytes()
            } else if data_file_id == writer_file_id {
                let scan = CheckpointBoundaryScan::writer(boundary_file, writer_next_record_id);
                let scan = self.reconcile_checkpoint_boundary_data_file(scan).await?;
                scan.recovered_bytes()
            } else if let Some(data_file_size) = self.data_file_size(&data_file_path).await? {
                // Middle files sit strictly inside the effective checkpointed live window, so every
                // byte in them is unread and checkpoint-live. They do not need boundary record
                // scanning.
                data_file_size
            } else {
                error!(
                    data_file_id,
                    "Missing checkpointed middle data file; treating records as lost."
                );
                0
            };
        }

        Ok(unread_buffer_size)
    }

    /// Lists the checkpoint-window files in reader-to-writer order.
    ///
    /// File IDs alone cannot distinguish a completely full, wrapped window from the temporary
    /// state where the reader is exactly one file ahead of the writer after abandoning a bad file.
    /// Iterating only IDs that exist avoids probing every possible file ID in the latter case while
    /// retaining every file in a genuinely full window. The writer boundary is always included so
    /// a missing boundary file still receives normal recovery handling.
    async fn checkpoint_data_file_ids(
        &self,
        reader_file_id: u16,
        writer_file_id: u16,
    ) -> Result<Vec<u16>, ReaderError<T>> {
        let mut data_file_ids = self
            .ledger()
            .filesystem()
            .list_files(&self.ledger().config().data_dir)
            .await
            .map_err(|source| ReaderError::Io { source })?
            .into_iter()
            .filter_map(|data_file_path| parse_data_file_id(&data_file_path))
            .filter(|&data_file_id| {
                data_file_id_in_range(data_file_id, reader_file_id, writer_file_id)
            })
            .collect::<Vec<_>>();

        data_file_ids.sort_unstable_by_key(|&data_file_id| {
            let start = u32::from(reader_file_id);
            (u32::from(data_file_id) + u32::from(MAX_FILE_ID) - start) % u32::from(MAX_FILE_ID)
        });
        data_file_ids.dedup();

        if data_file_ids.last() != Some(&writer_file_id) {
            data_file_ids.push(writer_file_id);
        }

        Ok(data_file_ids)
    }

    /// Finds the first checkpointed file that may contain records the reader has not acknowledged.
    async fn find_checkpoint_reader_file_id(
        &self,
        checkpoint_data_file_ids: &[u16],
        writer_file_id: u16,
        reader_last_record_id: u64,
    ) -> Result<u16, ReaderError<T>> {
        // File IDs bound the checkpointed live window, but the reader file ID may lag the
        // record-level checkpoint when file completion was not flushed before shutdown. Walk from
        // the durable reader file toward the writer and stop at the first file that may still
        // contain unread data.
        for &data_file_id in checkpoint_data_file_ids {
            if data_file_id == writer_file_id {
                // The writer file is the final file in the durable checkpoint window. Even if all
                // of its records are already acknowledged, the caller must scan it as a boundary
                // file so any post-checkpoint tail can be truncated.
                return Ok(data_file_id);
            }

            let data_file_path = self.ledger().get_data_file_path(data_file_id);
            match self
                .classify_checkpoint_data_file(data_file_id, &data_file_path)
                .await?
            {
                DataFileClassification::Missing | DataFileClassification::Empty => {}
                DataFileClassification::KnownLastRecord { last_record_id }
                    if last_record_id <= reader_last_record_id =>
                {
                    debug!(
                        data_file_id,
                        reader_last_record_id,
                        last_record_id,
                        "Data file is fully acknowledged; excluding it from recovered buffer size."
                    );
                }
                DataFileClassification::KnownLastRecord { .. } => return Ok(data_file_id),
                DataFileClassification::NeedsBoundaryScan => {
                    let boundary_file = CheckpointBoundaryFile {
                        id: data_file_id,
                        path: &data_file_path,
                    };
                    let scan = CheckpointBoundaryScan::reader(boundary_file, reader_last_record_id);
                    let scan = self.reconcile_checkpoint_boundary_data_file(scan).await?;
                    let unread_bytes = scan.recovered_bytes();
                    if unread_bytes > 0 {
                        return Ok(data_file_id);
                    }
                }
            }
        }

        Ok(writer_file_id)
    }

    /// Uses the final record to cheaply determine whether a file is fully acknowledged or needs
    /// scanning.
    async fn classify_checkpoint_data_file(
        &self,
        data_file_id: u16,
        data_file_path: &Path,
    ) -> Result<DataFileClassification, ReaderError<T>> {
        let data_file_mmap = match self
            .ledger()
            .filesystem()
            .open_mmap_readable(data_file_path)
            .await
        {
            Ok(data_file_mmap) => data_file_mmap,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                error!(
                    data_file_id,
                    "Missing checkpointed data file before unread boundary; treating records as lost."
                );
                return Ok(DataFileClassification::Missing);
            }
            Err(e) => return Err(ReaderError::Io { source: e }),
        };

        if data_file_mmap.as_ref().is_empty() {
            return Ok(DataFileClassification::Empty);
        }

        match validate_record_archive(data_file_mmap.as_ref(), &Hasher::new()) {
            RecordStatus::Valid { id: last_record_id } => {
                let record = try_as_record_archive(data_file_mmap.as_ref())
                    .expect("record was already validated");
                let item = match decode_record_payload::<T>(record) {
                    Ok(item) => item,
                    Err(error) => {
                        warn!(
                            data_file_id,
                            %error,
                            "Final checkpoint record could not be decoded; scanning file boundary."
                        );
                        return Ok(DataFileClassification::NeedsBoundaryScan);
                    }
                };
                let record_events =
                    u64::try_from(item.event_count()).expect("event count should never exceed u64");
                Ok(DataFileClassification::KnownLastRecord {
                    last_record_id: last_record_id + record_events.saturating_sub(1),
                })
            }
            RecordStatus::Corrupted { .. } => {
                warn!(
                    data_file_id,
                    "Final checkpoint record has an invalid checksum; scanning file boundary."
                );
                Ok(DataFileClassification::NeedsBoundaryScan)
            }
            RecordStatus::FailedDeserialization(error) => {
                warn!(
                    data_file_id,
                    ?error,
                    "Final checkpoint record could not be deserialized; scanning file boundary."
                );
                Ok(DataFileClassification::NeedsBoundaryScan)
            }
        }
    }

    /// Scans a checkpoint boundary, applying whichever reader and writer bounds it carries.
    async fn reconcile_checkpoint_boundary_data_file<'a>(
        &self,
        mut scan: CheckpointBoundaryScan<'a>,
    ) -> Result<CheckpointBoundaryScan<'a>, ReaderError<T>> {
        let Some(data_file) = self.open_checkpoint_boundary_data_file(scan.file).await? else {
            return Ok(scan);
        };

        let mut record_reader = RecordReader::new(data_file);

        while let Some(token) = self
            .try_next_checkpoint_record(&mut record_reader, &mut scan)
            .await?
        {
            let record_id = token.record_id();
            if self.truncate_if_post_checkpoint(&scan, record_id).await? {
                break;
            }

            let record = Self::decode_checkpoint_record(&mut record_reader, token, &scan);
            if self.truncate_if_checkpoint_crossing(&scan, &record).await? {
                break;
            }

            scan.include_record(&record);
        }

        Ok(scan)
    }

    /// Opens a boundary file while treating a missing checkpointed file as lost rather than fatal.
    async fn open_checkpoint_boundary_data_file(
        &self,
        file: CheckpointBoundaryFile<'_>,
    ) -> Result<Option<FS::File>, ReaderError<T>> {
        match self
            .ledger()
            .filesystem()
            .open_file_readable(file.path)
            .await
        {
            Ok(data_file) => Ok(Some(data_file)),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                error!(
                    data_file_id = file.id,
                    "Missing checkpointed boundary data file; treating records as lost."
                );
                Ok(None)
            }
            Err(e) => Err(ReaderError::Io { source: e }),
        }
    }

    /// Reads the next checkpoint record, preserving complete corrupt frames while truncating torn
    /// tails at the last complete record boundary.
    async fn try_next_checkpoint_record(
        &self,
        record_reader: &mut RecordReader<FS::File, T>,
        scan: &mut CheckpointBoundaryScan<'_>,
    ) -> Result<Option<ReadToken>, ReaderError<T>> {
        loop {
            match record_reader.try_next_record(true).await {
                Ok(Some(token)) => return Ok(Some(token)),
                Ok(None) => return Ok(None),
                Err(e) => {
                    if let Some(record_bytes) = e.consumed_record_bytes() {
                        if scan.corrupt_record_is_provably_post_checkpoint() {
                            let record_start_offset = scan.current_offset();
                            warn!(
                                data_file_id = scan.file.id,
                                truncate_at = record_start_offset,
                                record_bytes,
                                error = %e,
                                "Corrupt record is beyond writer checkpoint; truncating file."
                            );
                            self.truncate_data_file(scan.file.path, record_start_offset)
                                .await
                                .map_err(|source| ReaderError::Io { source })?;
                            return Ok(None);
                        }

                        warn!(
                            data_file_id = scan.file.id,
                            record_start_offset = scan.current_offset(),
                            record_bytes,
                            error = %e,
                            "Preserving complete corrupt checkpoint record."
                        );
                        scan.include_corrupt_record(record_bytes);
                        continue;
                    }

                    if e.is_bad_read() {
                        let record_start_offset = scan.current_offset();
                        warn!(
                            data_file_id = scan.file.id,
                            truncate_at = record_start_offset,
                            error = %e,
                            "Torn checkpoint tail; truncating file."
                        );
                        self.truncate_data_file(scan.file.path, record_start_offset)
                            .await
                            .map_err(|source| ReaderError::Io { source })?;
                        return Ok(None);
                    }

                    return Err(e);
                }
            }
        }
    }

    /// Extracts checkpoint metadata while retaining validated byte spans for undecodable payloads.
    fn decode_checkpoint_record(
        record_reader: &mut RecordReader<FS::File, T>,
        token: ReadToken,
        scan: &CheckpointBoundaryScan<'_>,
    ) -> CheckpointRecord {
        let id = token.record_id();
        let bytes = token.record_bytes() as u64;
        let record = match record_reader.read_record(token) {
            Ok(record) => record,
            Err(error) => {
                warn!(
                    data_file_id = scan.file.id,
                    record_id = id,
                    record_bytes = bytes,
                    %error,
                    "Checkpoint record payload is undecodable; counting its bytes."
                );
                return CheckpointRecord::undecodable(id, bytes);
            }
        };
        let events: u64 = record
            .event_count()
            .try_into()
            .expect("Event count for a record cannot exceed 2^64 events.");
        if events == 0 {
            warn!(
                data_file_id = scan.file.id,
                record_id = id,
                record_bytes = bytes,
                "Checkpoint record payload is empty; counting its bytes."
            );
            return CheckpointRecord::undecodable(id, bytes);
        }

        let next_id = id + events;
        let last_id = next_id - 1;

        CheckpointRecord {
            id,
            bytes,
            events: Some(events),
            next_id: Some(next_id),
            last_id: Some(last_id),
        }
    }

    /// Truncates before a record whose starting ID is outside the durable writer checkpoint.
    async fn truncate_if_post_checkpoint(
        &self,
        scan: &CheckpointBoundaryScan<'_>,
        record_id: u64,
    ) -> Result<bool, ReaderError<T>> {
        let Some(writer_next_record_id) = scan.writer_next_record_id else {
            return Ok(false);
        };
        if record_id < writer_next_record_id {
            return Ok(false);
        }

        debug!(
            data_file_id = scan.file.id,
            writer_next_record_id,
            record_id,
            truncate_at = scan.current_offset(),
            "Record starts beyond writer checkpoint; truncating file."
        );
        self.truncate_data_file(scan.file.path, scan.current_offset())
            .await
            .map_err(|source| ReaderError::Io { source })?;
        Ok(true)
    }

    /// Truncates before a multi-event record that straddles the durable writer checkpoint.
    async fn truncate_if_checkpoint_crossing(
        &self,
        scan: &CheckpointBoundaryScan<'_>,
        record: &CheckpointRecord,
    ) -> Result<bool, ReaderError<T>> {
        let Some(writer_next_record_id) = scan.writer_next_record_id else {
            return Ok(false);
        };
        let Some(next_id) = record.next_id else {
            return Ok(false);
        };
        if next_id <= writer_next_record_id {
            return Ok(false);
        }

        warn!(
            data_file_id = scan.file.id,
            writer_next_record_id,
            record_id = record.id,
            record_events = record
                .events
                .expect("a record with a next ID must have an event count"),
            truncate_at = scan.current_offset(),
            "Record crosses writer checkpoint; truncating file."
        );
        self.truncate_data_file(scan.file.path, scan.current_offset())
            .await
            .map_err(|source| ReaderError::Io { source })?;
        Ok(true)
    }

    /// Returns the physical file size, or `None` when the checkpointed file is missing.
    async fn data_file_size(&self, data_file_path: &Path) -> Result<Option<u64>, ReaderError<T>> {
        match self
            .ledger()
            .filesystem()
            .open_file_readable(data_file_path)
            .await
        {
            Ok(data_file) => data_file
                .metadata()
                .await
                .map(|metadata| Some(metadata.len()))
                .map_err(|source| ReaderError::Io { source }),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ReaderError::Io { source: e }),
        }
    }

    /// Durably truncates a data file so recovery and later appends observe the same physical EOF.
    async fn truncate_data_file(&self, data_file_path: &Path, size: u64) -> io::Result<()> {
        let data_file = self
            .ledger()
            .filesystem()
            .open_file_writable(data_file_path)
            .await?;
        data_file.truncate(size).await?;
        data_file.sync_all().await
    }
}

#[cfg(test)]
mod tests {
    use std::{path::Path, sync::Arc};

    use tokio::io::AsyncWriteExt;
    use vector_common::finalizer::OrderedFinalizer;

    use crate::{
        Bufferable,
        buffer_usage_data::BufferUsageHandle,
        test::{MultiEventRecord, SizedRecord, UndecodableRecord, with_temp_dir},
        variants::disk_v2::{
            DiskBufferConfigBuilder, Ledger, ProductionFilesystem, common::MAX_FILE_ID,
            writer::RecordWriter,
        },
    };

    use super::*;

    struct RecoveryFixture<T>
    where
        T: Bufferable,
    {
        reader: BufferReader<T, ProductionFilesystem>,
        ledger: Arc<Ledger<ProductionFilesystem>>,
    }

    impl<T> RecoveryFixture<T>
    where
        T: Bufferable,
    {
        async fn new(data_dir: &Path) -> Self {
            let config = DiskBufferConfigBuilder::from_path(data_dir)
                .build()
                .expect("creating buffer config should not fail");
            let ledger = Ledger::load_or_create(config, BufferUsageHandle::noop())
                .await
                .expect("ledger should not fail to load/create");
            let ledger = Arc::new(ledger);
            let (finalizer, _finalizer_stream) = OrderedFinalizer::new(None);
            let reader = BufferReader::new(Arc::clone(&ledger), finalizer);

            Self { reader, ledger }
        }

        async fn recover_unread_bytes(&self) -> u64 {
            self.reader
                .reconcile_checkpoint_window()
                .await
                .expect("recovery should not fail")
        }

        fn set_checkpoint(
            &self,
            reader_file_id: u16,
            writer_file_id: u16,
            reader_last_record_id: u64,
            writer_next_record_id: u64,
        ) {
            self.set_reader_file_id(reader_file_id);
            self.set_writer_file_id(writer_file_id);

            let current_reader_last_record_id = self.ledger.state().get_last_reader_record_id();
            assert!(
                current_reader_last_record_id <= reader_last_record_id,
                "test fixture cannot move reader checkpoint record ID backward"
            );
            self.ledger.state().increment_last_reader_record_id(
                reader_last_record_id - current_reader_last_record_id,
            );

            let current_writer_next_record_id = self.ledger.state().get_next_writer_record_id();
            assert!(
                current_writer_next_record_id <= writer_next_record_id,
                "test fixture cannot move writer checkpoint record ID backward"
            );
            self.ledger.state().increment_next_writer_record_id(
                writer_next_record_id - current_writer_next_record_id,
            );
        }

        fn set_reader_file_id(&self, target_file_id: u16) {
            while self.ledger.get_current_reader_file_id() != target_file_id {
                self.ledger.increment_acked_reader_file_id();
            }
            self.ledger.publish_cleanup_reader_file_id_for_test();
        }

        fn set_writer_file_id(&self, target_file_id: u16) {
            while self.ledger.get_current_writer_file_id() != target_file_id {
                self.ledger.state().increment_writer_file_id();
            }
        }

        async fn write_record_values<I>(&self, data_file_id: u16, records: I) -> Vec<u64>
        where
            I: IntoIterator<Item = (u64, T)>,
        {
            let data_file_path = self.ledger.get_data_file_path(data_file_id);
            let data_file = self
                .ledger
                .filesystem()
                .open_file_writable_atomic(&data_file_path)
                .await
                .expect("data file should be created");
            let mut writer = RecordWriter::new(
                data_file,
                0,
                self.ledger.config().write_buffer_size,
                self.ledger.config().max_data_file_size,
                self.ledger.config().max_record_size,
            );
            let mut record_bytes = Vec::new();

            for (record_id, record) in records {
                let (bytes_written, flush_result) = writer
                    .write_record(record_id, record)
                    .await
                    .expect("record write should not fail");
                assert_eq!(None, flush_result);
                record_bytes
                    .push(u64::try_from(bytes_written).expect("record size should fit into u64"));
            }

            writer.flush().await.expect("flush should not fail");
            writer.sync_all().await.expect("sync should not fail");

            record_bytes
        }

        async fn append_data_file_bytes(&self, data_file_id: u16, bytes: &[u8]) {
            let data_file_path = self.ledger.get_data_file_path(data_file_id);
            let mut data_file = self
                .ledger
                .filesystem()
                .open_file_writable(&data_file_path)
                .await
                .expect("data file should open");
            data_file
                .write_all(bytes)
                .await
                .expect("data file append should not fail");
            data_file.sync_all().await.expect("sync should not fail");
        }

        async fn create_empty_data_file(&self, data_file_id: u16) {
            let data_file_path = self.ledger.get_data_file_path(data_file_id);
            let data_file = self
                .ledger
                .filesystem()
                .open_file_writable_atomic(&data_file_path)
                .await
                .expect("data file should be created");
            data_file.sync_all().await.expect("sync should not fail");
        }

        async fn data_file_size(&self, data_file_id: u16) -> u64 {
            let data_file_path = self.ledger.get_data_file_path(data_file_id);
            self.ledger
                .filesystem()
                .open_file_readable(&data_file_path)
                .await
                .expect("data file should open")
                .metadata()
                .await
                .expect("metadata should load")
                .len()
        }

        async fn data_file_exists(&self, data_file_id: u16) -> bool {
            let data_file_path = self.ledger.get_data_file_path(data_file_id);
            tokio::fs::try_exists(data_file_path)
                .await
                .expect("checking data file existence should not fail")
        }
    }

    impl RecoveryFixture<SizedRecord> {
        async fn write_sized_records(&self, data_file_id: u16, records: &[(u64, u32)]) -> Vec<u64> {
            self.write_record_values(
                data_file_id,
                records
                    .iter()
                    .map(|&(record_id, payload_size)| (record_id, SizedRecord::new(payload_size))),
            )
            .await
        }
    }

    impl RecoveryFixture<MultiEventRecord> {
        async fn write_multi_event_records(
            &self,
            data_file_id: u16,
            records: &[(u64, u32)],
        ) -> Vec<u64> {
            self.write_record_values(
                data_file_id,
                records.iter().map(|&(record_id, event_count)| {
                    (record_id, MultiEventRecord::new(event_count))
                }),
            )
            .await
        }
    }

    #[tokio::test]
    async fn same_file_window_counts_undecodable_record_bytes() {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();

            async move {
                let fixture = RecoveryFixture::<UndecodableRecord>::new(&data_dir).await;
                let record_bytes = fixture
                    .write_record_values(0, [(1, UndecodableRecord)])
                    .await;
                fixture.set_checkpoint(0, 0, 0, 2);

                assert_eq!(record_bytes[0], fixture.recover_unread_bytes().await);
                assert_eq!(record_bytes[0], fixture.data_file_size(0).await);

                fixture.set_checkpoint(0, 0, 1, 2);
                assert_eq!(0, fixture.recover_unread_bytes().await);
                assert_eq!(record_bytes[0], fixture.data_file_size(0).await);
            }
        })
        .await;
    }

    #[tokio::test]
    async fn same_file_window_counts_only_unread_checkpointed_records() {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();

            async move {
                let fixture = RecoveryFixture::<SizedRecord>::new(&data_dir).await;
                let record_bytes = fixture
                    .write_sized_records(0, &[(1, 64), (2, 65), (3, 66), (4, 67)])
                    .await;

                fixture.set_checkpoint(0, 0, 1, 4);

                let expected_unread_bytes = record_bytes[1] + record_bytes[2];
                let expected_file_size = record_bytes[0] + expected_unread_bytes;

                assert_eq!(expected_unread_bytes, fixture.recover_unread_bytes().await);
                assert_eq!(expected_file_size, fixture.data_file_size(0).await);
            }
        })
        .await;
    }

    #[tokio::test]
    async fn reader_boundary_file_excludes_acknowledged_prefix() {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();

            async move {
                let fixture = RecoveryFixture::new(&data_dir).await;
                let reader_record_bytes = fixture
                    .write_sized_records(0, &[(1, 64), (2, 65), (3, 66)])
                    .await;
                fixture.create_empty_data_file(1).await;

                fixture.set_checkpoint(0, 1, 1, 4);

                let expected_unread_bytes = reader_record_bytes[1] + reader_record_bytes[2];

                assert_eq!(expected_unread_bytes, fixture.recover_unread_bytes().await);
                assert_eq!(
                    reader_record_bytes.iter().sum::<u64>(),
                    fixture.data_file_size(0).await
                );
            }
        })
        .await;
    }

    #[tokio::test]
    async fn writer_boundary_file_excludes_post_checkpoint_tail() {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();

            async move {
                let fixture = RecoveryFixture::new(&data_dir).await;
                fixture.create_empty_data_file(0).await;
                let writer_record_bytes = fixture
                    .write_sized_records(1, &[(4, 64), (5, 65), (6, 66)])
                    .await;

                fixture.set_checkpoint(0, 1, 3, 6);

                let expected_unread_bytes = writer_record_bytes[0] + writer_record_bytes[1];

                assert_eq!(expected_unread_bytes, fixture.recover_unread_bytes().await);
                assert_eq!(expected_unread_bytes, fixture.data_file_size(1).await);
            }
        })
        .await;
    }

    #[tokio::test]
    async fn writer_boundary_file_truncates_record_crossing_checkpoint() {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();

            async move {
                let fixture = RecoveryFixture::<MultiEventRecord>::new(&data_dir).await;
                fixture.create_empty_data_file(0).await;
                let writer_record_bytes = fixture
                    .write_multi_event_records(1, &[(1, 1), (2, 3), (5, 1)])
                    .await;

                fixture.set_checkpoint(0, 1, 0, 4);

                let expected_unread_bytes = writer_record_bytes[0];

                assert_eq!(expected_unread_bytes, fixture.recover_unread_bytes().await);
                assert_eq!(expected_unread_bytes, fixture.data_file_size(1).await);
            }
        })
        .await;
    }

    #[tokio::test]
    async fn torn_tail_is_truncated_to_last_valid_record_boundary() {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();

            async move {
                let fixture = RecoveryFixture::new(&data_dir).await;
                let record_bytes = fixture.write_sized_records(0, &[(1, 64), (2, 65)]).await;
                let expected_file_size = record_bytes.iter().sum::<u64>();
                fixture.append_data_file_bytes(0, &[0x01, 0x02, 0x03]).await;
                assert_eq!(expected_file_size + 3, fixture.data_file_size(0).await);

                fixture.set_checkpoint(0, 0, 0, 3);

                assert_eq!(expected_file_size, fixture.recover_unread_bytes().await);
                assert_eq!(expected_file_size, fixture.data_file_size(0).await);
            }
        })
        .await;
    }

    #[tokio::test]
    async fn middle_data_file_uses_full_file_size() {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();

            async move {
                let fixture = RecoveryFixture::new(&data_dir).await;
                fixture.create_empty_data_file(0).await;
                let middle_record_bytes = fixture
                    .write_sized_records(1, &[(4, 64), (5, 65), (6, 66)])
                    .await;
                fixture.create_empty_data_file(2).await;

                fixture.set_checkpoint(0, 2, 3, 7);

                let expected_unread_bytes = middle_record_bytes.iter().sum::<u64>();

                assert_eq!(expected_unread_bytes, fixture.recover_unread_bytes().await);
                assert_eq!(expected_unread_bytes, fixture.data_file_size(1).await);
            }
        })
        .await;
    }

    #[tokio::test]
    async fn full_wrapped_checkpoint_window_counts_every_data_file() {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();

            async move {
                let fixture = RecoveryFixture::new(&data_dir).await;
                let mut expected_unread_bytes = 0;
                for data_file_id in 0..MAX_FILE_ID {
                    let record_id = u64::from(data_file_id) + 1;
                    expected_unread_bytes += fixture
                        .write_sized_records(data_file_id, &[(record_id, 64)])
                        .await[0];
                }

                // reader == next(writer) is ambiguous: unlike the empty reader-ahead recovery
                // state, every file ID exists here and all of their records are unread.
                fixture.set_checkpoint(0, MAX_FILE_ID - 1, 0, u64::from(MAX_FILE_ID) + 1);

                assert_eq!(expected_unread_bytes, fixture.recover_unread_bytes().await);
            }
        })
        .await;
    }

    #[tokio::test]
    async fn missing_boundary_data_files_contribute_no_bytes() {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();

            async move {
                let fixture = RecoveryFixture::<SizedRecord>::new(&data_dir).await;

                fixture.set_checkpoint(0, 0, 0, 1);
                assert_eq!(0, fixture.recover_unread_bytes().await);
                assert!(!fixture.data_file_exists(0).await);

                fixture.set_checkpoint(0, 1, 0, 1);
                assert_eq!(0, fixture.recover_unread_bytes().await);
                assert!(!fixture.data_file_exists(0).await);
                assert!(!fixture.data_file_exists(1).await);
            }
        })
        .await;
    }

    #[tokio::test]
    async fn missing_middle_data_file_contributes_no_bytes() {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();

            async move {
                let fixture = RecoveryFixture::<SizedRecord>::new(&data_dir).await;
                fixture.create_empty_data_file(0).await;
                fixture.create_empty_data_file(2).await;

                fixture.set_checkpoint(0, 2, 3, 7);

                assert_eq!(0, fixture.recover_unread_bytes().await);
            }
        })
        .await;
    }

    #[tokio::test]
    async fn missing_middle_data_files_do_not_prevent_recovery() {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();

            async move {
                let fixture = RecoveryFixture::new(&data_dir).await;
                let reader_record_bytes = fixture.write_sized_records(0, &[(1, 64), (2, 65)]).await;
                let first_existing_middle_record_bytes =
                    fixture.write_sized_records(2, &[(5, 66), (6, 67)]).await;
                let second_existing_middle_record_bytes =
                    fixture.write_sized_records(4, &[(9, 68), (10, 69)]).await;
                let writer_record_bytes = fixture
                    .write_sized_records(5, &[(11, 70), (12, 71), (13, 72)])
                    .await;

                assert!(!fixture.data_file_exists(1).await);
                assert!(!fixture.data_file_exists(3).await);

                fixture.set_checkpoint(0, 5, 1, 13);

                let expected_unread_bytes = reader_record_bytes[1]
                    + first_existing_middle_record_bytes.iter().sum::<u64>()
                    + second_existing_middle_record_bytes.iter().sum::<u64>()
                    + writer_record_bytes[0]
                    + writer_record_bytes[1];
                let expected_writer_file_size = writer_record_bytes[0] + writer_record_bytes[1];

                assert_eq!(expected_unread_bytes, fixture.recover_unread_bytes().await);
                assert!(!fixture.data_file_exists(1).await);
                assert!(!fixture.data_file_exists(3).await);
                assert_eq!(
                    first_existing_middle_record_bytes.iter().sum::<u64>(),
                    fixture.data_file_size(2).await
                );
                assert_eq!(
                    second_existing_middle_record_bytes.iter().sum::<u64>(),
                    fixture.data_file_size(4).await
                );
                assert_eq!(expected_writer_file_size, fixture.data_file_size(5).await);
            }
        })
        .await;
    }

    #[tokio::test]
    async fn stale_data_file_cleanup_deletes_files_outside_checkpoint_window() {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();

            async move {
                let fixture = RecoveryFixture::<SizedRecord>::new(&data_dir).await;
                for data_file_id in 0..MAX_FILE_ID {
                    fixture.create_empty_data_file(data_file_id).await;
                }

                fixture.set_checkpoint(2, 4, 3, 7);

                assert_eq!(
                    3,
                    fixture
                        .ledger
                        .cleanup_stale_data_files()
                        .await
                        .expect("cleanup should not fail")
                );
                assert!(!fixture.data_file_exists(0).await);
                assert!(!fixture.data_file_exists(1).await);
                assert!(fixture.data_file_exists(2).await);
                assert!(fixture.data_file_exists(3).await);
                assert!(fixture.data_file_exists(4).await);
                assert!(!fixture.data_file_exists(5).await);
            }
        })
        .await;
    }

    #[tokio::test]
    async fn stale_data_file_cleanup_uses_last_published_reader_checkpoint() {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();

            async move {
                let fixture = RecoveryFixture::<SizedRecord>::new(&data_dir).await;
                for data_file_id in 0..MAX_FILE_ID {
                    fixture.create_empty_data_file(data_file_id).await;
                }

                fixture.set_checkpoint(1, 4, 3, 7);

                fixture.ledger.increment_acked_reader_file_id();
                fixture.ledger.increment_acked_reader_file_id();

                assert_eq!(
                    2,
                    fixture
                        .ledger
                        .cleanup_stale_data_files()
                        .await
                        .expect("cleanup should not fail")
                );
                assert!(!fixture.data_file_exists(0).await);
                assert!(fixture.data_file_exists(1).await);
                assert!(fixture.data_file_exists(2).await);
                assert!(fixture.data_file_exists(3).await);
                assert!(fixture.data_file_exists(4).await);
                assert!(!fixture.data_file_exists(5).await);

                fixture.ledger.publish_cleanup_reader_file_id_for_test();

                assert_eq!(
                    2,
                    fixture
                        .ledger
                        .cleanup_stale_data_files()
                        .await
                        .expect("cleanup should not fail")
                );
                assert!(!fixture.data_file_exists(1).await);
                assert!(!fixture.data_file_exists(2).await);
                assert!(fixture.data_file_exists(3).await);
                assert!(fixture.data_file_exists(4).await);
            }
        })
        .await;
    }

    #[tokio::test]
    async fn recovery_should_not_count_fully_acknowledged_middle_files() {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();

            async move {
                let fixture = RecoveryFixture::<SizedRecord>::new(&data_dir).await;
                let acknowledged_reader_record_bytes =
                    fixture.write_sized_records(0, &[(1, 64), (2, 65)]).await;
                let acknowledged_middle_record_bytes =
                    fixture.write_sized_records(1, &[(3, 66), (4, 67)]).await;
                let unread_middle_record_bytes =
                    fixture.write_sized_records(2, &[(5, 68), (6, 69)]).await;
                let writer_record_bytes = fixture.write_sized_records(3, &[(7, 70), (8, 71)]).await;

                fixture.set_checkpoint(0, 3, 4, 9);

                let expected_unread_bytes = unread_middle_record_bytes.iter().sum::<u64>()
                    + writer_record_bytes.iter().sum::<u64>();

                assert_eq!(expected_unread_bytes, fixture.recover_unread_bytes().await);
                assert_eq!(
                    acknowledged_reader_record_bytes.iter().sum::<u64>(),
                    fixture.data_file_size(0).await
                );
                assert_eq!(
                    acknowledged_middle_record_bytes.iter().sum::<u64>(),
                    fixture.data_file_size(1).await
                );
            }
        })
        .await;
    }

    #[tokio::test]
    async fn recovery_skips_torn_fully_acknowledged_files_before_unread_start() {
        with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();

            async move {
                let fixture = RecoveryFixture::<SizedRecord>::new(&data_dir).await;
                let acknowledged_reader_record_bytes =
                    fixture.write_sized_records(0, &[(1, 64), (2, 65)]).await;
                fixture.append_data_file_bytes(0, &[0x01, 0x02, 0x03]).await;
                let acknowledged_middle_record_bytes =
                    fixture.write_sized_records(1, &[(3, 66), (4, 67)]).await;
                let unread_middle_record_bytes =
                    fixture.write_sized_records(2, &[(5, 68), (6, 69)]).await;
                let writer_record_bytes = fixture.write_sized_records(3, &[(7, 70), (8, 71)]).await;

                fixture.set_checkpoint(0, 3, 4, 9);

                let expected_unread_bytes = unread_middle_record_bytes.iter().sum::<u64>()
                    + writer_record_bytes.iter().sum::<u64>();

                assert_eq!(expected_unread_bytes, fixture.recover_unread_bytes().await);
                assert_eq!(
                    acknowledged_reader_record_bytes.iter().sum::<u64>(),
                    fixture.data_file_size(0).await
                );
                assert_eq!(
                    acknowledged_middle_record_bytes.iter().sum::<u64>(),
                    fixture.data_file_size(1).await
                );
            }
        })
        .await;
    }
}
