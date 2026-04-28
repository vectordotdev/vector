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
    finalization::{AddBatchNotifier, BatchNotifier},
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
async fn reader_throws_error_when_record_length_delimiter_is_zero() {
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

            // Now reopen the buffer and attempt a read, which should return an error for
            // deserialization failure, but specifically that the record length was zero.
            let (_, mut reader, _) = create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            match reader.next().await {
                Err(ReaderError::Deserialization { reason }) => {
                    assert!(reason.ends_with("record length was zero"));
                }
                _ => panic!("read_result should be deserialization error"),
            }
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

            // Now reopen the buffer.  We should get a good read, a failed read, and then a final
            // good read:
            // - first read is the first record, nothing special
            // - second read is an error because we detect a partial record write which can't be
            //   read as a valid record, forcing us to skip to the second data file
            // - third read is the third record that we successfully wrote to the second data file
            let (mut writer, mut reader, ledger) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            writer.close();
            assert_reader_writer_v2_file_positions!(ledger, 0, 1);

            let first_read = await_timeout!(reader.next(), 2).expect("read should not fail");
            assert_eq!(first_read, Some(SizedRecord::new(first_record_size)));
            assert_reader_writer_v2_file_positions!(ledger, 0, 1);
            acknowledge(first_read.unwrap()).await;

            let second_read = await_timeout!(reader.next(), 2).expect_err("read should fail");
            assert!(matches!(second_read, ReaderError::PartialWrite));
            assert_reader_writer_v2_file_positions!(ledger, 1, 1);

            let third_read = await_timeout!(reader.next(), 2).expect("read should not fail");
            assert_eq!(third_read, Some(SizedRecord::new(third_record_size)));
            assert_reader_writer_v2_file_positions!(ledger, 1, 1);
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

// TODO: Update this test, and the other "reader throws error when" tests to assert that the data
// file is immediately deleted on the next call to `next`.
#[tokio::test]
async fn reader_throws_error_when_record_has_scrambled_archive_data() {
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

            // Now reopen the buffer and attempt a read, which should return an error for
            // deserialization failure.
            let (_writer, mut reader, _ledger) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            let read_result = reader.next().await;
            assert!(matches!(
                read_result,
                Err(ReaderError::Deserialization { .. })
            ));
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
            let (mut writer, mut reader, _ledger) = create_default_buffer_v2(data_dir).await;

            // Write an `UndecodableRecord` record which will encode correctly, but always throw an
            // error when attempting to decode.
            writer
                .write_record(UndecodableRecord)
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");

            // Now try to read it back, which should return an error.
            let read_result = reader.next().await;
            assert!(matches!(read_result, Err(ReaderError::Decode { .. })));
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
            let marked_for_skip = assertion_registry
                .build()
                .with_name("mark_for_skip")
                .with_parent_name("writer_detects_when_last_record_has_scrambled_archive_data")
                .was_entered()
                .finalize();

            // Create a regular buffer, no customizations required.
            let (mut writer, _, ledger) = create_default_buffer_v2(data_dir.clone()).await;
            let starting_writer_file_id = ledger.get_current_writer_file_id();
            let expected_final_writer_file_id = ledger.get_next_writer_file_id();
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

            // We should not have seen a call to `mark_for_skip` yet.
            assert!(!marked_for_skip.try_assert());

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

    let parent = trace_span!("writer_detects_when_last_record_has_scrambled_archive_data");
    fut.instrument(parent.or_current()).await;
}

#[tokio::test]
async fn writer_detects_when_last_record_has_invalid_checksum() {
    let assertion_registry = install_tracing_helpers();
    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let marked_for_skip = assertion_registry
                .build()
                .with_name("mark_for_skip")
                .with_parent_name("writer_detects_when_last_record_has_invalid_checksum")
                .was_entered()
                .finalize();

            // Create a regular buffer, no customizations required.
            let (mut writer, _, ledger) = create_default_buffer_v2(data_dir.clone()).await;
            let starting_writer_file_id = ledger.get_current_writer_file_id();
            let expected_final_writer_file_id = ledger.get_next_writer_file_id();
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

            // We should not have seen a call to `mark_for_skip` yet.
            assert!(!marked_for_skip.try_assert());

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
            let actual_writer_next_record_id = ledger.state().get_next_writer_record_id();

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

            // Now reopen the buffer, which should trigger the skip ahead logic where we move our
            // writer next record ID to be ahead of the actual last record ID, but on whatever we
            // pulled out of the data file.  This is required to maintain our monotonicity invariant
            // for all records written into the buffer.
            let (_, _, ledger) = create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            writer_did_not_call_reset.assert();
            assert_reader_writer_v2_file_positions!(ledger, 0, expected_final_writer_file_id);
            assert_file_does_not_exist_async!(&expected_final_write_data_file);
            assert_eq!(
                actual_writer_next_record_id,
                ledger.state().get_next_writer_record_id()
            );
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
    marked_for_skip: Assertion,
    data_file_path: PathBuf,
    starting_writer_file_id: u16,
    expected_final_writer_file_id: u16,
    expected_final_write_data_file: PathBuf,
    expected_data_file_len: u64,
}

async fn write_two_records_and_read_all_then_drop(
    data_dir: PathBuf,
    assertion_registry: &AssertionRegistry,
) -> ScrambledTestSetup {
    let marked_for_skip = assertion_registry
        .build()
        .with_name("mark_for_skip")
        .with_parent_name("writer_and_reader_handle_when_last_record_has_scrambled_archive_data")
        .was_entered()
        .finalize();

    let (mut writer, mut reader, ledger) = create_default_buffer_v2(data_dir.clone()).await;

    let starting_writer_file_id = ledger.get_current_writer_file_id();
    let expected_final_writer_file_id = ledger.get_next_writer_file_id();
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
        marked_for_skip,
        data_file_path: ledger.get_current_writer_data_file_path(),
        starting_writer_file_id,
        expected_final_writer_file_id,
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
                marked_for_skip,
                data_file_path,
                starting_writer_file_id,
                expected_final_writer_file_id,
                expected_final_write_data_file,
                expected_data_file_len,
            } = write_two_records_and_read_all_then_drop(data_dir.clone(), &assertion_registry)
                .await;

            // We should not have seen a call to `mark_for_skip` yet.
            assert!(!marked_for_skip.try_assert());

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

            // Now reopen the buffer, which should trigger a `Writer::mark_for_skip` call which
            // instructs the writer to skip to the next data file, although this doesn't happen
            // until the first write is attempted.
            let (mut writer, mut reader, ledger) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir).await;
            marked_for_skip.assert();
            // When writer see last record as corrupted set flag to skip to next file but reader moves to next file id and wait for writer to create it.
            assert_reader_writer_v2_file_positions!(
                ledger,
                expected_final_writer_file_id,
                starting_writer_file_id
            );
            assert_file_does_not_exist_async!(&expected_final_write_data_file);

            // At this point reader is waiting for writer to create next data file, so we can test that reader.next() times out.
            let result = timeout(Duration::from_millis(100), reader.next()).await;
            assert!(result.is_err(), "expected reader.next() to time out");

            // Do a simple write to ensure it opens the next data file.
            let _bytes_written = writer
                .write_record(SizedRecord::new(72))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");
            assert_reader_writer_v2_file_positions!(
                ledger,
                expected_final_writer_file_id,
                expected_final_writer_file_id
            );
            assert_file_exists_async!(&expected_final_write_data_file);

            let read = reader
                .next()
                .await
                .expect("should not fail to read record")
                .expect("should contain first record");
            assert_eq!(SizedRecord::new(72), read);
            acknowledge(read).await;
        }
    });

    let parent =
        trace_span!("writer_and_reader_handle_when_last_record_has_scrambled_archive_data");
    fut.instrument(parent.or_current()).await;
}

/// Reproducer for <https://github.com/vectordotdev/vector/issues/18336>
///
/// # Bug Description
///
/// When a Vector process is killed mid-write (e.g., by OOM killer), it may leave a partial write
/// at the end of a disk buffer data file. A "partial write" here means the 8-byte length delimiter
/// was flushed to disk but the actual record data was not (because it was still in the OS page
/// cache or the writer's internal buffer when the process died).
///
/// On restart, `validate_last_write()` correctly detects the corrupted last record and marks the
/// writer to skip to the next data file (`skip_to_next = true`). However, the ledger's
/// `writer_current_data_file` is NOT updated at this point — it is only updated lazily when the
/// writer actually opens the next file (i.e., on the first successful write after restart).
///
/// Meanwhile, `seek_to_next_record()` initializes the reader to the last acknowledged position.
/// If there were valid but unacknowledged records BEFORE the partial write, the reader's seek
/// stops short of the corrupted region. After seek completes, `ready_to_read = true`, and the
/// reader and writer both appear to be on the same file ID (both = 0 from the ledger).
///
/// During normal operation, the `is_finalized` flag is computed as:
/// ```text
/// is_finalized = (reader_file_id != writer_file_id) || !self.ready_to_read
/// ```
/// Since both IDs are 0 and `ready_to_read = true`, `is_finalized = false`.
///
/// When the reader eventually reaches the partial write, it reads the 8-byte length delimiter
/// (claiming 1024 bytes follow), then enters the `try_next_record` inner loop. Since the file is
/// at EOF, `fill_buf()` returns an empty buffer. But because `is_finalized = false`, the code
/// does NOT return a `PartialWrite` error — it busy-spins forever, never making progress. This
/// manifests as ~100% CPU on a single core and the affected sink stopping all output.
///
/// # Test Setup
///
/// 1. Write two records (A and B), flushing both to disk.
/// 2. Read and acknowledge only record A (leaving B as valid but unacknowledged).
/// 3. Flush the ledger to persist the acknowledgement (`reader_last_record_id = 0`).
/// 4. Simulate a crash by appending only the 8-byte length delimiter of a new record (no data).
/// 5. Reopen the buffer (simulate restart).
///    - `validate_last_write()` detects `FailedDeserialization`, calls `mark_for_skip()`.
///    - Ledger still says `writer_current_data_file = 0` (not updated yet).
///    - `seek_to_next_record()`: `ledger_last = 0`, `last_reader_record_id starts at 0`,
///      condition `0 < 0` is false, so seek does nothing. Reader positioned at start of file 0.
/// 6. Normal reads: record A succeeds, record B succeeds, then the reader hits the partial write.
/// 7. With the bug: `is_finalized = false`, so the third read busy-spins (timeout fires = FAIL).
/// 8. With a fix: the third read returns `Err(ReaderError::PartialWrite)` within the timeout.
#[tokio::test]
async fn reader_hangs_after_partial_write_beyond_last_acked_record() {
    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a buffer with default settings.
            let (mut writer, mut reader, ledger) = create_default_buffer_v2(data_dir.clone()).await;

            // Write two records (A and B) and flush them both to disk.
            writer
                .write_record(SizedRecord::new(64))
                .await
                .expect("write A should not fail");
            writer.flush().await.expect("flush should not fail");

            writer
                .write_record(SizedRecord::new(64))
                .await
                .expect("write B should not fail");
            writer.flush().await.expect("flush should not fail");

            // Read and acknowledge only record A. Record B remains valid but unacknowledged.
            // The ledger's `reader_last_record_id` is set to 0 (record A's ID).
            let record_a = reader
                .next()
                .await
                .expect("should not fail to read")
                .expect("should contain record A");
            assert_eq!(SizedRecord::new(64), record_a);
            acknowledge(record_a).await;

            // Flush the ledger to persist the acknowledgement.
            // After this point: ledger `reader_last_record_id = 0`.
            ledger.flush().expect("should not fail to flush ledger");

            let data_file_path = ledger.get_current_writer_data_file_path();
            drop(reader);
            drop(writer);
            drop(ledger);

            // Simulate a process crash: append only the 8-byte length delimiter of a new record,
            // with no record data following. The value 1024 claims "1024 bytes of data follow",
            // but nothing does. This mimics what happens when the OS flushes the write buffer
            // partially before the process is killed.
            {
                let mut data_file = OpenOptions::new()
                    .append(true)
                    .open(&data_file_path)
                    .await
                    .expect("should not fail to open data file for appending");
                let fake_record_length: u64 = 1024;
                data_file
                    .write_all(&fake_record_length.to_be_bytes())
                    .await
                    .expect("should not fail to write fake length delimiter");
                data_file.flush().await.expect("flush should not fail");
                data_file.sync_all().await.expect("sync should not fail");
            }

            // Reopen the buffer, simulating a process restart. During `Buffer::from_config_inner`:
            //
            //   1. `validate_last_write()` mmaps the data file. `check_archived_root::<Record>`
            //      fails on the last 8 bytes (not a valid archived Record structure) and returns
            //      FailedDeserialization. The writer calls `reset()` + `mark_for_skip()`.
            //      Crucially, the ledger's `writer_current_data_file` remains 0.
            //
            //   2. `seek_to_next_record()` with `ledger_last = 0`. Since `last_reader_record_id`
            //      starts at 0 and the loop condition is `0 < 0 = false`, the seek loop does not
            //      execute. Reader is positioned at the beginning of file 0. `ready_to_read = true`.
            let (_, mut reader, _) = create_default_buffer_v2::<_, SizedRecord>(data_dir).await;

            // Record A: valid record at the start of file 0. Should succeed.
            // `is_finalized = false` (reader_file_id=0 == writer_file_id=0), but there is real
            // data to read so fill_buf() returns data immediately rather than spinning.
            let read_a =
                await_timeout!(reader.next(), 2).expect("reading record A should not fail");
            assert_eq!(read_a, Some(SizedRecord::new(64)));

            // Record B: valid record after A in file 0. Should also succeed.
            let read_b =
                await_timeout!(reader.next(), 2).expect("reading record B should not fail");
            assert_eq!(read_b, Some(SizedRecord::new(64)));

            // Third read: hits the partial write (8-byte length delimiter with no data).
            //
            // With the bug:
            //   - `is_finalized = false` because reader_file_id (0) == writer_file_id (0).
            //     The writer has `skip_to_next = true` internally but has NOT yet updated the
            //     ledger (the update happens lazily on the first actual write).
            //   - The reader calls `try_next_record(false)`: reads the 8-byte delimiter (1024),
            //     then enters the inner loop trying to read 1024 bytes.
            //   - `fill_buf()` returns empty (EOF) but `is_finalized = false` means no
            //     PartialWrite error is returned. The reader busy-spins indefinitely.
            //   - Each `fill_buf()` briefly suspends the task via tokio's blocking thread pool
            //     (the underlying tokio::fs::File dispatches reads there), so the timeout CAN fire.
            //   - After 2 seconds the timeout fires and `third_read` is `Err(Elapsed)`.
            //   - The assertion below fails: bug confirmed.
            //
            // With a fix, `is_finalized` should become true when the reader detects it is behind
            // the writer, and the read should return `Err(ReaderError::PartialWrite)` promptly.
            let third_read = timeout(Duration::from_secs(2), reader.next()).await;
            assert!(
                third_read.is_ok(),
                "reader timed out when reading past a partial write — \
                 is_finalized was false because the ledger's writer_current_data_file \
                 was not updated by validate_last_write() \
                 (reproducer for https://github.com/vectordotdev/vector/issues/18336)"
            );
        }
    });

    fut.await;
}
