use bytes::{Buf, BufMut};
use memmap2::MmapMut;
use std::{
    io::{self, SeekFrom},
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};
use tokio::{
    fs::OpenOptions,
    io::{AsyncSeekExt, AsyncWriteExt},
    time::{Duration, timeout},
};
use tracing::Instrument;
use tracing_fluent_assertions::{Assertion, AssertionRegistry};
use vector_common::{
    byte_size_of::ByteSizeOf,
    finalization::{
        AddBatchNotifier, BatchNotifier, EventFinalizers, Finalizable, MergeFinalizable,
    },
};

use super::{create_buffer_v2_with_max_data_file_size, create_default_buffer_v2};
use crate::{
    EventCount, assert_buffer_size, assert_enough_bytes_written, assert_file_does_not_exist_async,
    assert_file_exists_async, assert_reader_writer_v2_file_positions, await_timeout,
    encoding::{AsMetadata, Encodable},
    test::{SizedRecord, UndecodableRecord, acknowledge, install_tracing_helpers, with_temp_dir},
    variants::disk_v2::{ReaderError, backed_archive::BackedArchive, record::Record},
};

impl AsMetadata for u32 {
    fn into_u32(self) -> u32 {
        self
    }

    fn from_u32(value: u32) -> Option<Self> {
        if value < 32 { Some(value) } else { None }
    }
}

#[tokio::test]
async fn startup_truncates_record_length_delimiter_that_is_zero() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, _, ledger) = create_default_buffer_v2(data_dir.clone()).await;

            // Write a normal `SizedRecord` record.
            let bytes_written = writer
                .write_record(SizedRecord::new(64))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");

            let expected_data_file_len = bytes_written as u64;

            // Grab the current writer data file path, and then drop the writer/reader.  Once the
            // buffer is closed, we'll purposefully zero out the length delimiter, which should
            // make `RecordReader` angry.
            let data_file_path = ledger.get_current_writer_data_file_path();
            drop(writer);
            drop(ledger);

            // Open the file and zero out the first four bytes.
            let mut data_file = OpenOptions::new()
                .write(true)
                .open(&data_file_path)
                .await
                .expect("open should not fail");

            // Just to make sure the data file matches our expected state before futzing with it.
            let metadata = data_file
                .metadata()
                .await
                .expect("metadata should not fail");
            assert_eq!(expected_data_file_len, metadata.len());

            let pos = data_file
                .seek(SeekFrom::Start(0))
                .await
                .expect("seek should not fail");
            assert_eq!(0, pos);
            data_file
                .write_all(&[0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0])
                .await
                .expect("write should not fail");
            data_file.flush().await.expect("flush should not fail");
            data_file.sync_all().await.expect("sync should not fail");
            drop(data_file);

            // Now reopen the buffer. Startup recovery treats the durable ledger checkpoint as
            // authoritative, so the corrupted record is truncated before the reader is handed out.
            let (_, mut reader, _) = create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            let data_file = OpenOptions::new()
                .read(true)
                .open(&data_file_path)
                .await
                .expect("open should not fail");
            let metadata = data_file
                .metadata()
                .await
                .expect("metadata should not fail");
            assert_eq!(0, metadata.len());

            let read_result = reader.next().await.expect("read should not fail");
            assert_eq!(read_result, None);
        }
    })
    .await;
}

#[tokio::test]
async fn reader_throws_error_when_finished_file_has_truncated_record_data() {
    // Right now, we _always_ assume the data is coming if we can at least read 8 bytes for the
    // length delimiter... but the point in the code where that happens is oblivious to the
    // higher-level reader/writer state, so if there was an error that lead to a data file ending
    // prematurely, the underlying reader would not be aware of this and would wait forever for
    // however many bytes.
    //
    // This is actually a higher-level problem insofar as we'll willingly continue trying to read
    // out a record even if there's only one byte left, because the contract is that when a data
    // file is done, and we've read all the records, there should be no bytes left over... which is
    // a reasonable invariant!
    //
    // If there's at least one more byte, though, or if there was a record that took up, say, 1000
    // bytes in theory but only 999 bytes got written and the writer has moved on, we'll sit there
    // forever waiting for that last byte before we move on to the next data file.
    //
    // Thus, what we want to test for is to ensure that when the writer _has_ moved on, and there's
    // not enough data to possibly continue, we correctly detect this situation and move on.  All of
    // our existing logic -- checking bytes read vs file size when deleting, checking record ID gap
    // when updating last read record ID -- should handle keeping the buffer size accurate as well
    // as detecting corrupted records, so there should be no issue there.
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a buffer with a smaller-than-normal data file size limit, just so that we can
            // force the writer to roll to another data file and then easily mess with the previous
            // data file.
            let (mut writer, _, ledger) =
                create_buffer_v2_with_max_data_file_size(data_dir.clone(), 172).await;

            // Write two smaller records, such that the first one fits entirely, and the second one
            // starts within the 128-byte zone but finishes over the limit, thus triggering data
            // file rollover.
            let first_record_size = 32;
            let first_bytes_written = writer
                .write_record(SizedRecord::new(first_record_size))
                .await
                .expect("write should not fail");
            let second_record_size = 33;
            let second_bytes_written = writer
                .write_record(SizedRecord::new(second_record_size))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");

            let expected_first_data_file_len = first_bytes_written + second_bytes_written;
            let first_data_file_path = ledger.get_current_writer_data_file_path();

            // Make sure we're in the right state before doing a third write, which should land in
            // another data file.
            assert_buffer_size!(ledger, 2, expected_first_data_file_len);
            assert_reader_writer_v2_file_positions!(ledger, 0, 0);

            // Do our third write, which should land in a new data file.
            let third_record_size = 34;
            let third_bytes_written = writer
                .write_record(SizedRecord::new(third_record_size))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");

            assert_buffer_size!(
                ledger,
                3,
                expected_first_data_file_len + third_bytes_written
            );
            assert_reader_writer_v2_file_positions!(ledger, 0, 1);

            // Now drop the writer/ledger to close the buffer, so we can do some hackin' and
            // slashin' to the first data file. >:D
            drop(writer);
            drop(ledger);

            // Open the file and truncate it so that we can read the length delimiter of the second
            // record, but only part of the second record itself.
            let mut data_file = OpenOptions::new()
                .write(true)
                .open(&first_data_file_path)
                .await
                .expect("open should not fail");

            // Just to make sure the data file matches our expected state before futzing with it.
            let metadata = data_file
                .metadata()
                .await
                .expect("metadata should not fail");
            assert_eq!(expected_first_data_file_len as u64, metadata.len());

            // Middle of the second record seems good.
            data_file
                .set_len((first_bytes_written + (second_bytes_written / 2)) as u64)
                .await
                .expect("truncating should not fail");
            data_file.flush().await.expect("flush should not fail");
            data_file.sync_all().await.expect("sync should not fail");
            drop(data_file);

            // Now reopen the buffer. Startup recovery truncates the torn tail of the first data
            // file to the last valid record boundary, so runtime sees the first valid record and
            // then the valid record in the next data file without surfacing the startup corruption
            // as a reader error.
            let (mut writer, mut reader, ledger) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            writer.close();
            assert_reader_writer_v2_file_positions!(ledger, 0, 1);

            let data_file = OpenOptions::new()
                .read(true)
                .open(&first_data_file_path)
                .await
                .expect("open should not fail");
            let metadata = data_file
                .metadata()
                .await
                .expect("metadata should not fail");
            assert_eq!(first_bytes_written as u64, metadata.len());

            let first_read = await_timeout!(reader.next(), 2).expect("read should not fail");
            assert_eq!(first_read, Some(SizedRecord::new(first_record_size)));
            assert_reader_writer_v2_file_positions!(ledger, 0, 1);
            acknowledge(first_read.unwrap()).await;

            let second_read = await_timeout!(reader.next(), 2).expect("read should not fail");
            assert_eq!(second_read, Some(SizedRecord::new(third_record_size)));
            assert_reader_writer_v2_file_positions!(ledger, 1, 1);
            acknowledge(second_read.unwrap()).await;

            let final_read = await_timeout!(reader.next(), 2).expect("read should not fail");
            assert_eq!(final_read, None);
            assert_reader_writer_v2_file_positions!(ledger, 1, 1);
        }
    })
    .await;
}

#[tokio::test]
async fn reader_accounts_abandoned_tail_when_runtime_bad_read_rolls_file() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let (mut writer, mut reader, ledger) =
                create_buffer_v2_with_max_data_file_size(data_dir.clone(), 172).await;

            let first_record_size = 32;
            let first_bytes_written = writer
                .write_record(SizedRecord::new(first_record_size))
                .await
                .expect("write should not fail");
            let second_record_size = 33;
            let second_bytes_written = writer
                .write_record(SizedRecord::new(second_record_size))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");

            let first_data_file_len = first_bytes_written + second_bytes_written;
            let first_data_file_path = ledger.get_current_writer_data_file_path();
            assert_buffer_size!(ledger, 2, first_data_file_len);
            assert_reader_writer_v2_file_positions!(ledger, 0, 0);

            let third_record_size = 34;
            let third_bytes_written = writer
                .write_record(SizedRecord::new(third_record_size))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");

            assert_buffer_size!(ledger, 3, first_data_file_len + third_bytes_written);
            assert_reader_writer_v2_file_positions!(ledger, 0, 1);

            // Corrupt the already-open reader file after startup while preserving its length, so
            // the runtime bad-read path accounts for the whole abandoned unread tail rather than
            // startup recovery truncating it first.
            let mut data_file = OpenOptions::new()
                .write(true)
                .open(&first_data_file_path)
                .await
                .expect("open should not fail");
            assert_eq!(
                first_data_file_len as u64,
                data_file
                    .metadata()
                    .await
                    .expect("metadata should not fail")
                    .len()
            );
            let corrupt_offset = u64::try_from(first_bytes_written)
                .expect("record size should fit in u64")
                + 8;
            data_file
                .seek(SeekFrom::Start(corrupt_offset))
                .await
                .expect("seek should not fail");
            data_file
                .write_all(&[0xFF; 8])
                .await
                .expect("write should not fail");
            data_file.flush().await.expect("flush should not fail");
            data_file.sync_all().await.expect("sync should not fail");
            drop(data_file);
            writer.close();

            let first_read = await_timeout!(reader.next(), 2).expect("read should not fail");
            assert_eq!(first_read, Some(SizedRecord::new(first_record_size)));
            acknowledge(first_read.unwrap()).await;

            let expected_after_ack = second_bytes_written + third_bytes_written;
            let expected_after_bad_read = third_bytes_written;
            let bad_read = await_timeout!(reader.next(), 2).expect_err("read should fail");
            assert!(matches!(
                bad_read,
                ReaderError::Checksum { .. }
                    | ReaderError::Deserialization { .. }
                    | ReaderError::PartialWrite
            ));
            assert_eq!(
                expected_after_bad_read as u64,
                ledger.get_total_buffer_size(),
                "runtime bad-read recovery should subtract the unread abandoned tail from the buffer size"
            );
            assert!(
                ledger.get_total_buffer_size() < expected_after_ack as u64,
                "test should prove the abandoned-tail accounting changed the buffer size"
            );
            assert_reader_writer_v2_file_positions!(ledger, 1, 1);

            let third_read = await_timeout!(reader.next(), 2).expect("read should not fail");
            assert_eq!(third_read, Some(SizedRecord::new(third_record_size)));
            acknowledge(third_read.unwrap()).await;

            let final_read = await_timeout!(reader.next(), 2).expect("read should not fail");
            assert_eq!(final_read, None);
            assert_reader_writer_v2_file_positions!(ledger, 1, 1);
        }
    })
    .await;
}

// TODO: Add test that emulates "reader throws error when" such that we write three records, each to
// a separate data file, corrupt the write in the second data file, and make sure that we get our
// first and third record back and that after reading and acking the first and third record (plus
// one more read to trigger it) that we've deleted all three data files.

#[tokio::test]
async fn startup_truncates_file_when_record_has_scrambled_archive_data() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, _, ledger) = create_default_buffer_v2(data_dir.clone()).await;

            // Write two `SizedRecord` records just so we can generate enough data.  We need two
            // records because the writer, on start up, will specifically check the last record and
            // validate it.  If it's not valid, the data file is skipped entirely.  So we'll write
            // two records, and only scramble the first... which will let the reader be the one to
            // discover the error.
            let first_bytes_written = writer
                .write_record(SizedRecord::new(64))
                .await
                .expect("should not fail to write");
            writer.flush().await.expect("flush should not fail");
            let second_bytes_written = writer
                .write_record(SizedRecord::new(65))
                .await
                .expect("should not fail to write");
            writer.flush().await.expect("flush should not fail");

            let expected_data_file_len = first_bytes_written as u64 + second_bytes_written as u64;

            // Grab the current writer data file path, and then drop the writer/reader.  Once the
            // buffer is closed, we'll purposefully scramble the archived data -- but not the length
            // delimiter -- which should trigger `rkyv` to throw an error when we check the data.
            let data_file_path = ledger.get_current_writer_data_file_path();
            drop(writer);
            drop(ledger);

            // Open the file and set the last eight bytes of the first record to something clearly
            // wrong/invalid, which should end up messing with the relative pointer stuff in the
            // archive.
            let mut data_file = OpenOptions::new()
                .write(true)
                .open(&data_file_path)
                .await
                .expect("open should not fail");

            // Just to make sure the data file matches our expected state before futzing with it.
            let metadata = data_file
                .metadata()
                .await
                .expect("metadata should not fail");
            assert_eq!(expected_data_file_len, metadata.len());

            let target_pos = first_bytes_written as u64 - 8;
            let pos = data_file
                .seek(SeekFrom::Start(target_pos))
                .await
                .expect("seek should not fail");
            assert_eq!(target_pos, pos);
            data_file
                .write_all(&[0xd, 0xe, 0xa, 0xd, 0xb, 0xe, 0xe, 0xf])
                .await
                .expect("should not fail to write");
            data_file.flush().await.expect("flush should not fail");
            data_file.sync_all().await.expect("sync should not fail");
            drop(data_file);

            // Now reopen the buffer. Startup recovery discovers the corrupted first record and
            // truncates the file at offset zero, because records after a corrupted record are not
            // reachable through the normal sequential reader.
            let (mut writer, mut reader, ledger) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            writer.close();
            assert_eq!(0, ledger.get_total_buffer_size());

            let data_file = OpenOptions::new()
                .read(true)
                .open(&data_file_path)
                .await
                .expect("open should not fail");
            let metadata = data_file
                .metadata()
                .await
                .expect("metadata should not fail");
            assert_eq!(0, metadata.len());

            let read_result = reader.next().await.expect("read should not fail");
            assert_eq!(read_result, None);
        }
    })
    .await;
}

#[tokio::test]
async fn reader_throws_error_when_record_has_decoding_error() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, mut reader, ledger) = create_default_buffer_v2(data_dir).await;

            // Write an `UndecodableRecord` record which will encode correctly, but always throw an
            // error when attempting to decode.
            let bytes_written = writer
                .write_record(UndecodableRecord)
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");
            assert_eq!(bytes_written as u64, ledger.get_total_buffer_size());

            // Reading drops the record because its validated byte range cannot be decoded. Those
            // bytes cannot be acknowledged, so they must leave the logical buffer immediately.
            let read_result = reader.next().await;
            assert!(matches!(read_result, Err(ReaderError::Decode { .. })));
            assert_eq!(0, ledger.get_total_buffer_size());

            // Once the writer closes, the empty logical buffer must terminate instead of waiting
            // forever for bytes that were already dropped.
            drop(writer);
            let final_read = await_timeout!(reader.next(), 2).expect("read should not fail");
            assert_eq!(None, final_read);
        }
    })
    .await;
}

#[tokio::test]
async fn writer_detects_when_last_record_has_scrambled_archive_data() {
    let assertion_registry = install_tracing_helpers();
    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let writer_did_not_mark_for_skip = assertion_registry
                .build()
                .with_name("mark_for_skip")
                .with_parent_name("writer_detects_when_last_record_has_scrambled_archive_data")
                .was_not_entered()
                .finalize();

            // Create a regular buffer, no customizations required.
            let (mut writer, _, ledger) = create_default_buffer_v2(data_dir.clone()).await;
            let starting_writer_file_id = ledger.get_current_writer_file_id();
            let expected_final_write_data_file = ledger.get_next_writer_data_file_path();
            assert_file_does_not_exist_async!(&expected_final_write_data_file);

            // Write a `SizedRecord` record that we can scramble.  Since it will be the last record
            // in the data file, the writer should detect this error when the buffer is recreated,
            // even though it doesn't actually _emit_ anything we can observe when creating the
            // buffer... but it should trigger a call to `reset`, which we _can_ observe with
            // tracing assertions.
            let bytes_written = writer
                .write_record(SizedRecord::new(64))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");

            let expected_data_file_len = bytes_written as u64;

            // Grab the current writer data file path, and then drop the writer/reader.  Once the
            // buffer is closed, we'll purposefully scramble the archived data -- but not the length
            // delimiter -- which should trigger `rkyv` to throw an error when we check the data.
            let data_file_path = ledger.get_current_writer_data_file_path();
            drop(writer);
            drop(ledger);

            writer_did_not_mark_for_skip.assert();

            // Open the file and set the last eight bytes of the record to something clearly
            // wrong/invalid, which should end up messing with the relative pointer stuff in the
            // archive.
            let mut data_file = OpenOptions::new()
                .write(true)
                .open(&data_file_path)
                .await
                .expect("open should not fail");

            // Just to make sure the data file matches our expected state before futzing with it.
            let metadata = data_file
                .metadata()
                .await
                .expect("metadata should not fail");
            assert_eq!(expected_data_file_len, metadata.len());

            let target_pos = expected_data_file_len - 8;
            let pos = data_file
                .seek(SeekFrom::Start(target_pos))
                .await
                .expect("seek should not fail");
            assert_eq!(target_pos, pos);
            data_file
                .write_all(&[0xd, 0xe, 0xa, 0xd, 0xb, 0xe, 0xe, 0xf])
                .await
                .expect("write should not fail");
            data_file.flush().await.expect("flush should not fail");
            data_file.sync_all().await.expect("sync should not fail");
            drop(data_file);

            // Now reopen the buffer. The durable checkpoint is authoritative, so startup truncates
            // the corrupted tail and keeps writing in the current data file instead of marking the
            // writer to skip to the next one.
            let (mut writer, _, ledger) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            writer_did_not_mark_for_skip.assert();
            assert_reader_writer_v2_file_positions!(ledger, 0, starting_writer_file_id);
            assert_file_does_not_exist_async!(&expected_final_write_data_file);

            let data_file = OpenOptions::new()
                .read(true)
                .open(&data_file_path)
                .await
                .expect("open should not fail");
            let metadata = data_file
                .metadata()
                .await
                .expect("metadata should not fail");
            assert_eq!(0, metadata.len());

            // Do a simple write to ensure it continues in the current data file.
            let _bytes_written = writer
                .write_record(SizedRecord::new(64))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");
            assert_reader_writer_v2_file_positions!(ledger, 0, starting_writer_file_id);
            assert_file_does_not_exist_async!(&expected_final_write_data_file);
        }
    });

    let parent = trace_span!("writer_detects_when_last_record_has_scrambled_archive_data");
    fut.instrument(parent.or_current()).await;
}

#[tokio::test]
async fn writer_detects_when_last_record_has_invalid_checksum() {
    let assertion_registry = install_tracing_helpers();
    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let writer_did_not_mark_for_skip = assertion_registry
                .build()
                .with_name("mark_for_skip")
                .with_parent_name("writer_detects_when_last_record_has_invalid_checksum")
                .was_not_entered()
                .finalize();

            // Create a regular buffer, no customizations required.
            let (mut writer, _, ledger) = create_default_buffer_v2(data_dir.clone()).await;
            let starting_writer_file_id = ledger.get_current_writer_file_id();
            let expected_final_write_data_file = ledger.get_next_writer_data_file_path();
            assert_file_does_not_exist_async!(&expected_final_write_data_file);

            // Write a `SizedRecord` record that we can scramble.  Since it will be the last record
            // in the data file, the writer should detect this error when the buffer is recreated,
            // even though it doesn't actually _emit_ anything we can observe when creating the
            // buffer... but it should trigger a call to `reset`, which we _can_ observe with
            // tracing assertions.
            let bytes_written = writer
                .write_record(SizedRecord::new(13))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");

            let expected_data_file_len = bytes_written as u64;

            // Grab the current writer data file path, and then drop the writer/reader.  Once the
            // buffer is closed, we'll reload the record as a mutable archive so we can scramble the
            // data used by the checksum calculation, but not in a way that `rkyv` won't be able to
            // deserialize it.  This would simulate something more like a bit flip than a portion of
            // the data failing to be written entirely.
            let data_file_path = ledger.get_current_writer_data_file_path();
            drop(writer);
            drop(ledger);

            writer_did_not_mark_for_skip.assert();

            // Open the file, mutably deserialize the record, and flip a bit in the checksum.
            let data_file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&data_file_path)
                .await
                .expect("open should not fail");

            // Just to make sure the data file matches our expected state before futzing with it.
            let metadata = data_file
                .metadata()
                .await
                .expect("metadata should not fail");
            assert_eq!(expected_data_file_len, metadata.len());

            let std_data_file = data_file.into_std().await;
            let record_mmap =
                unsafe { MmapMut::map_mut(&std_data_file).expect("mmap should not fail") };
            drop(std_data_file);

            let mut backed_record = BackedArchive::<_, Record>::from_backing(record_mmap)
                .expect("archive should not fail");
            let record = backed_record.get_archive_mut();

            // Just flip the 15th bit.  Should be enough. *shrug*
            {
                let projected_checksum =
                    unsafe { record.map_unchecked_mut(|record| &mut record.checksum) };
                let projected_checksum = projected_checksum.get_mut();
                let new_checksum = *projected_checksum ^ (1 << 15);
                *projected_checksum = new_checksum;
            }

            // Flush the memory-mapped data file to disk and we're done with our modification.
            backed_record
                .get_backing_ref()
                .flush()
                .expect("flush should not fail");
            drop(backed_record);

            // Now reopen the buffer. The durable checkpoint is authoritative, so startup truncates
            // the corrupted tail and keeps writing in the current data file instead of marking the
            // writer to skip to the next one.
            let (mut writer, _, ledger) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            writer_did_not_mark_for_skip.assert();
            assert_reader_writer_v2_file_positions!(ledger, 0, starting_writer_file_id);
            assert_file_does_not_exist_async!(&expected_final_write_data_file);

            let data_file = OpenOptions::new()
                .read(true)
                .open(&data_file_path)
                .await
                .expect("open should not fail");
            let metadata = data_file
                .metadata()
                .await
                .expect("metadata should not fail");
            assert_eq!(0, metadata.len());

            // Do a simple write to ensure it continues in the current data file.
            let _bytes_written = writer
                .write_record(SizedRecord::new(64))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");
            assert_reader_writer_v2_file_positions!(ledger, 0, starting_writer_file_id);
            assert_file_does_not_exist_async!(&expected_final_write_data_file);
        }
    });

    let parent = trace_span!("writer_detects_when_last_record_has_invalid_checksum");
    fut.instrument(parent.or_current()).await;
}

#[tokio::test]
async fn writer_detects_when_last_record_wasnt_flushed() {
    let assertion_registry = install_tracing_helpers();
    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let marked_for_skip = assertion_registry
                .build()
                .with_name("mark_for_skip")
                .with_parent_name("writer_detects_when_last_record_wasnt_flushed")
                .was_entered()
                .finalize();

            // Create a regular buffer, no customizations required.
            let (mut writer, _, ledger) = create_default_buffer_v2(data_dir.clone()).await;
            let starting_writer_file_id = ledger.get_current_writer_file_id();
            let expected_final_writer_file_id = ledger.get_next_writer_file_id();
            let expected_final_write_data_file = ledger.get_next_writer_data_file_path();
            assert_file_does_not_exist_async!(&expected_final_write_data_file);

            // Write a regular record so something is in the data file.
            let bytes_written = writer
                .write_record(SizedRecord::new(64))
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(bytes_written, SizedRecord, 64);
            writer.flush().await.expect("flush should not fail");

            // Now unsafely increment the next writer record ID, which will cause a divergence
            // between the actual data file and the ledger.  Specifically, the code will think that
            // a record was written but never flushed, given that the next writer record ID has
            // advanced.  This represents a "lost write"/"corrupted events" scenario, where we end
            // up reporting that we missed a bunch of events, either because we skipped a file or
            // a bunch of writes never fully made it to disk.
            let writer_next_record_id = ledger.state().get_next_writer_record_id();
            unsafe {
                ledger
                    .state()
                    .unsafe_set_writer_next_record_id(writer_next_record_id + 1);
            }

            // Grab the current writer data file path, and then drop the writer/reader.
            drop(writer);
            drop(ledger);

            // We should not have seen a call to `mark_for_skip` yet.
            assert!(!marked_for_skip.try_assert());

            // Now reopen the buffer, which should trigger a `Writer::mark_for_skip` call which
            // instructs the writer to skip to the next data file, although this doesn't happen
            // until the first write is attempted.
            let (mut writer, _, ledger) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            marked_for_skip.assert();
            assert_reader_writer_v2_file_positions!(ledger, 0, starting_writer_file_id);
            assert_file_does_not_exist_async!(&expected_final_write_data_file);

            // Do a simple write to ensure it opens the next data file.
            let _bytes_written = writer
                .write_record(SizedRecord::new(64))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");
            assert_reader_writer_v2_file_positions!(ledger, 0, expected_final_writer_file_id);
            assert_file_exists_async!(&expected_final_write_data_file);
        }
    });

    let parent = trace_span!("writer_detects_when_last_record_wasnt_flushed");
    fut.instrument(parent.or_current()).await;
}

#[tokio::test]
async fn writer_detects_when_last_record_was_flushed_but_id_wasnt_incremented() {
    let assertion_registry = install_tracing_helpers();
    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let writer_did_not_call_reset = assertion_registry
                .build()
                .with_name("reset")
                .with_parent_name(
                    "writer_detects_when_last_record_was_flushed_but_id_wasnt_incremented",
                )
                .was_not_entered()
                .finalize();

            // Create a regular buffer, no customizations required.
            let (mut writer, _, ledger) = create_default_buffer_v2(data_dir.clone()).await;
            let starting_writer_next_record_id = ledger.state().get_next_writer_record_id();
            let expected_final_writer_file_id = ledger.get_current_writer_file_id();
            let expected_final_write_data_file = ledger.get_next_writer_data_file_path();
            assert_file_does_not_exist_async!(&expected_final_write_data_file);

            // Write a regular record so something is in the data file.
            let bytes_written = writer
                .write_record(SizedRecord::new(64))
                .await
                .expect("write should not fail");
            assert_enough_bytes_written!(bytes_written, SizedRecord, 64);
            writer.flush().await.expect("flush should not fail");
            let current_writer_data_file = ledger.get_current_writer_data_file_path();

            // Now unsafely decrement the next writer record ID, which will cause a divergence
            // between the actual data file and the ledger.  Specifically, the code will think that
            // a write made it to disk but that the process was stopped, or crashed, before it was
            // able to actually increment the writer next record ID, so a record ID will exist on
            // disk that it thinks should not exist, purely from the data we have in the ledger.
            unsafe {
                ledger
                    .state()
                    .unsafe_set_writer_next_record_id(starting_writer_next_record_id);
            }

            // Grab the current writer data file path, and then drop the writer/reader.
            drop(writer);
            drop(ledger);

            writer_did_not_call_reset.assert();

            // Now reopen the buffer. The durable checkpoint is authoritative, so startup should
            // truncate the post-checkpoint record instead of fast-forwarding the ledger to match
            // bytes that may not have been durable before the crash.
            let (_, _, ledger) = create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            writer_did_not_call_reset.assert();
            assert_reader_writer_v2_file_positions!(ledger, 0, expected_final_writer_file_id);
            assert_file_does_not_exist_async!(&expected_final_write_data_file);
            assert_eq!(
                starting_writer_next_record_id,
                ledger.state().get_next_writer_record_id()
            );

            let data_file = OpenOptions::new()
                .read(true)
                .open(&current_writer_data_file)
                .await
                .expect("open should not fail");
            let metadata = data_file
                .metadata()
                .await
                .expect("metadata should not fail");
            assert_eq!(0, metadata.len());
        }
    });

    let parent =
        trace_span!("writer_detects_when_last_record_was_flushed_but_id_wasnt_incremented");
    fut.instrument(parent.or_current()).await;
}

#[tokio::test]
async fn reader_throws_error_when_record_is_undecodable_via_metadata() {
    static GET_METADATA_VALUE: AtomicU32 = AtomicU32::new(0);
    static CAN_DECODE_VALUE: AtomicU32 = AtomicU32::new(0);

    #[derive(Debug)]
    struct ControllableRecord(u8);

    impl Encodable for ControllableRecord {
        type Metadata = u32;
        type EncodeError = io::Error;
        type DecodeError = io::Error;

        fn get_metadata() -> Self::Metadata {
            GET_METADATA_VALUE.load(Ordering::Relaxed)
        }

        fn can_decode(metadata: Self::Metadata) -> bool {
            CAN_DECODE_VALUE.load(Ordering::Relaxed) == metadata
        }

        fn encode<B: BufMut>(self, buffer: &mut B) -> Result<(), Self::EncodeError> {
            buffer.put_u8(self.0);
            Ok(())
        }

        fn decode<B: Buf>(_: Self::Metadata, mut buffer: B) -> Result<Self, Self::DecodeError> {
            let b = buffer.get_u8();
            Ok(ControllableRecord(b))
        }
    }

    impl AddBatchNotifier for ControllableRecord {
        fn add_batch_notifier(&mut self, batch: BatchNotifier) {
            drop(batch); // We never check acknowledgements for this type
        }
    }

    impl Finalizable for ControllableRecord {
        fn take_finalizers(&mut self) -> EventFinalizers {
            EventFinalizers::DEFAULT
        }
    }

    impl MergeFinalizable for ControllableRecord {
        fn merge_finalizers(&mut self, _finalizers: EventFinalizers) {
            // We never check acknowledgements for this type.
        }
    }

    impl ByteSizeOf for ControllableRecord {
        fn allocated_bytes(&self) -> usize {
            0
        }
    }

    impl EventCount for ControllableRecord {
        fn event_count(&self) -> usize {
            1
        }
    }

    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, mut reader, _ledger) = create_default_buffer_v2(data_dir).await;

            // Write two `ControllableRecord` records which will encode with metadata matching our
            // starting metadata state.  We'll then make sure we can read the first one out before
            // tweaking the value underpinning the `can_decode` logic.
            writer
                .write_record(ControllableRecord(21))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");

            writer
                .write_record(ControllableRecord(86))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");

            // Write one more `ControllableRecord` record but with an adjusted metadata value that
            // we'll make sure doesn't correctly convert from `u32` to `T::Metadata`.  This is to
            // exercise the codepath where the flags don't even seem to be valid at all i.e. bits
            // are set that aren't even defined on the Vector side.
            GET_METADATA_VALUE.store(33, Ordering::Relaxed);
            writer
                .write_record(ControllableRecord(54))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");

            // Now try to read back the first record, which should return correctly:
            let first_read_result = reader.next().await;
            assert!(matches!(
                first_read_result,
                Ok(Some(ControllableRecord(21)))
            ));

            // And now try to read back the second record, but first, we'll tweak `CAN_DECODE_VALUE`
            // so that it doesn't match the metadata value the second record was encoded with, which
            // should cause an "incompatible" error:
            CAN_DECODE_VALUE.store(1, Ordering::Relaxed);
            let second_read_result = reader.next().await;
            assert!(matches!(second_read_result, Err(ReaderError::Incompatible { .. })));

            let ReaderError::Incompatible { reason: second_read_error_reason } = second_read_result.unwrap_err() else {
                panic!("error should be ReadError::Incompatible");
            };

            let expected_second_read_error_reason = format!("record metadata not supported (metadata: {:#036b})", 0_u32);
            assert_eq!(expected_second_read_error_reason, second_read_error_reason);

            // And finally we try to read back the third record, which shouldn't even get to the
            // `can_decode` step because the metadata value just couldn't be converted:
            // And now try to read back the second record, but first, we'll tweak `CAN_DECODE_VALUE`
            // so that it doesn't match the metadata value the second record was encoded with, which
            // should cause an "incompatible" error:
            let third_read_result = reader.next().await;
            assert!(matches!(third_read_result, Err(ReaderError::Incompatible { .. })));
            let ReaderError::Incompatible { reason: third_read_error_reason } = third_read_result.unwrap_err() else {
                panic!("error should be ReadError::Incompatible");
            };

            let expected_third_read_error_reason_prefix = "invalid metadata for";
            assert!(third_read_error_reason.starts_with(expected_third_read_error_reason_prefix),
                "error reason when metadata cannot be converted should start with 'metadata invalid for', got '{third_read_error_reason}' instead");
        }
    })
    .await;
}

struct ScrambledTestSetup {
    writer_did_not_mark_for_skip: Assertion,
    data_file_path: PathBuf,
    starting_writer_file_id: u16,
    expected_final_write_data_file: PathBuf,
    expected_data_file_len: u64,
}

async fn write_two_records_and_read_all_then_drop(
    data_dir: PathBuf,
    assertion_registry: &AssertionRegistry,
) -> ScrambledTestSetup {
    let writer_did_not_mark_for_skip = assertion_registry
        .build()
        .with_name("mark_for_skip")
        .with_parent_name("writer_and_reader_handle_when_last_record_has_scrambled_archive_data")
        .was_not_entered()
        .finalize();

    let (mut writer, mut reader, ledger) = create_default_buffer_v2(data_dir.clone()).await;

    let starting_writer_file_id = ledger.get_current_writer_file_id();
    let expected_final_write_data_file = ledger.get_next_writer_data_file_path();
    assert_file_does_not_exist_async!(&expected_final_write_data_file);

    let bytes_written_1 = writer
        .write_record(SizedRecord::new(64))
        .await
        .expect("write failed");
    let bytes_written_2 = writer
        .write_record(SizedRecord::new(68))
        .await
        .expect("write failed");
    writer.flush().await.expect("flush failed");
    writer.close();

    let expected_data_file_len = bytes_written_1 + bytes_written_2;

    let first_read = reader
        .next()
        .await
        .expect("read failed")
        .expect("missing record");
    assert_eq!(SizedRecord::new(64), first_read);
    acknowledge(first_read).await;

    let second_read = reader
        .next()
        .await
        .expect("read failed")
        .expect("missing record");
    assert_eq!(SizedRecord::new(68), second_read);
    acknowledge(second_read).await;

    let third_read = reader.next().await.expect("read failed");
    assert!(third_read.is_none());

    ledger.flush().expect("flush failed");

    ScrambledTestSetup {
        writer_did_not_mark_for_skip,
        data_file_path: ledger.get_current_writer_data_file_path(),
        starting_writer_file_id,
        expected_final_write_data_file,
        expected_data_file_len: expected_data_file_len as u64,
    }
}

#[tokio::test]
async fn writer_and_reader_handle_when_last_record_has_scrambled_archive_data() {
    let assertion_registry = install_tracing_helpers();
    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let ScrambledTestSetup {
                writer_did_not_mark_for_skip,
                data_file_path,
                starting_writer_file_id,
                expected_final_write_data_file,
                expected_data_file_len,
            } = write_two_records_and_read_all_then_drop(data_dir.clone(), &assertion_registry)
                .await;

            writer_did_not_mark_for_skip.assert();

            // Open the file and set the last eight bytes of the record to something clearly
            // wrong/invalid, which should end up messing with the relative pointer stuff in the
            // archive.
            let mut data_file = OpenOptions::new()
                .write(true)
                .open(&data_file_path)
                .await
                .expect("open should not fail");

            // Just to make sure the data file matches our expected state before futzing with it.
            let metadata = data_file
                .metadata()
                .await
                .expect("metadata should not fail");
            assert_eq!(expected_data_file_len, metadata.len());

            let target_pos = expected_data_file_len - 8;
            let pos = data_file
                .seek(SeekFrom::Start(target_pos))
                .await
                .expect("seek should not fail");
            assert_eq!(target_pos, pos);
            data_file
                .write_all(&[0xd, 0xe, 0xa, 0xd, 0xb, 0xe, 0xe, 0xf])
                .await
                .expect("write should not fail");
            data_file.flush().await.expect("flush should not fail");
            data_file.sync_all().await.expect("sync should not fail");
            drop(data_file);

            // Now reopen the buffer. Startup recovery truncates the corrupted writer tail to the
            // last valid record boundary and keeps the writer on the current file.
            let (mut writer, mut reader, ledger) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            writer_did_not_mark_for_skip.assert();
            assert_reader_writer_v2_file_positions!(
                ledger,
                starting_writer_file_id,
                starting_writer_file_id
            );
            assert_file_does_not_exist_async!(&expected_final_write_data_file);

            // At this point there are no unread bytes, so reader.next() should still wait.
            let result = timeout(Duration::from_millis(100), reader.next()).await;
            assert!(result.is_err(), "expected reader.next() to time out");

            // Do a simple write to ensure it appends to the current data file.
            let _bytes_written = writer
                .write_record(SizedRecord::new(72))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");
            assert_reader_writer_v2_file_positions!(
                ledger,
                starting_writer_file_id,
                starting_writer_file_id
            );
            assert_file_does_not_exist_async!(&expected_final_write_data_file);

            let read = reader
                .next()
                .await
                .expect("should not fail to read record")
                .expect("should contain first record");
            assert_eq!(SizedRecord::new(72), read);
            assert_reader_writer_v2_file_positions!(
                ledger,
                starting_writer_file_id,
                starting_writer_file_id
            );
            acknowledge(read).await;
        }
    });

    let parent =
        trace_span!("writer_and_reader_handle_when_last_record_has_scrambled_archive_data");
    fut.instrument(parent.or_current()).await;
}
