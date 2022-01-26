use std::{
    io::{self, SeekFrom},
    sync::atomic::{AtomicU32, Ordering},
};

use bytes::{Buf, BufMut};
use memmap2::MmapMut;
use tokio::{
    fs::OpenOptions,
    io::{AsyncSeekExt, AsyncWriteExt},
};
use tracing::Instrument;
use vector_common::byte_size_of::ByteSizeOf;

use super::{create_default_buffer, install_tracing_helpers, with_temp_dir, UndecodableRecord};
use crate::{
    assert_buffer_size, assert_enough_bytes_written, assert_file_does_not_exist_async,
    assert_file_exists_async, assert_reader_writer_file_positions, await_timeout,
    disk_v2::{
        backed_archive::BackedArchive,
        record::Record,
        tests::{create_buffer_with_max_data_file_size, SizedRecord},
        ReaderError,
    },
    encoding::{AsMetadata, Encodable},
};

#[tokio::test]
async fn reader_throws_error_when_record_length_delimiter_is_zero() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, _, _, ledger) = create_default_buffer(data_dir.clone()).await;

            // Write a normal `SizedRecord` record.
            let bytes_written = writer
                .write_record(SizedRecord(64))
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
            let (_, mut reader, _, _) = create_default_buffer::<_, SizedRecord>(data_dir).await;
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
            let (mut writer, _, _, ledger) =
                create_buffer_with_max_data_file_size(data_dir.clone(), 128).await;

            // Write two smaller records, such that the first one fits entirelyh, and the second one
            // starts within the 128-byte zone but finishes over the limit, thus triggering data
            // file rollover.
            let first_bytes_written = writer
                .write_record(SizedRecord(62))
                .await
                .expect("write should not fail");
            let second_bytes_written = writer
                .write_record(SizedRecord(63))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");

            let expected_first_data_file_len = first_bytes_written + second_bytes_written;
            let first_data_file_path = ledger.get_current_writer_data_file_path();

            // Make sure we're in the right state before doing a third write, which should land in
            // another data file.
            assert_buffer_size!(ledger, 2, expected_first_data_file_len);
            assert_reader_writer_file_positions!(ledger, 0, 0);

            // Do our third write, which should land in a new data file.
            let third_bytes_written = writer
                .write_record(SizedRecord(64))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");

            assert_buffer_size!(
                ledger,
                3,
                expected_first_data_file_len + third_bytes_written
            );
            assert_reader_writer_file_positions!(ledger, 0, 1);

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
            let (mut writer, mut reader, acker, ledger) =
                create_default_buffer::<_, SizedRecord>(data_dir).await;
            writer.close();
            assert_reader_writer_file_positions!(ledger, 0, 1);

            let first_read = await_timeout!(reader.next(), 2).expect("read should not fail");
            assert_eq!(first_read, Some(SizedRecord(62)));
            assert_reader_writer_file_positions!(ledger, 0, 1);
            acker.ack(1);

            let second_read = await_timeout!(reader.next(), 2).expect_err("read should fail");
            assert!(matches!(second_read, ReaderError::PartialWrite));
            assert_reader_writer_file_positions!(ledger, 1, 1);

            let third_read = await_timeout!(reader.next(), 2).expect("read should not fail");
            assert_eq!(third_read, Some(SizedRecord(64)));
            assert_reader_writer_file_positions!(ledger, 1, 1);
            acker.ack(1);

            let final_read = await_timeout!(reader.next(), 2).expect("read should not fail");
            assert_eq!(final_read, None);
            assert_reader_writer_file_positions!(ledger, 1, 1);
        }
    })
    .await;
}

#[tokio::test]
async fn reader_throws_error_when_record_has_scrambled_archive_data() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, _, _, ledger) = create_default_buffer(data_dir.clone()).await;

            // Write two `SizedRecord` records just so we can generate enough data.  We need two
            // records because the writer, on start up, will specifically check the last record and
            // validate it.  If it's not valid, the data file is skipped entirely.  So we'll write
            // two records, and only scramble the first... which will let the reader be the one to
            // discover the error.
            let first_bytes_written = writer
                .write_record(SizedRecord(64))
                .await
                .expect("should not fail to write");
            writer.flush().await.expect("flush should not fail");
            let second_bytes_written = writer
                .write_record(SizedRecord(65))
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
            let (_writer, mut reader, _acker, _ledger) =
                create_default_buffer::<_, SizedRecord>(data_dir).await;
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
            let (mut writer, mut reader, _acker, _ledger) = create_default_buffer(data_dir).await;

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
            let (mut writer, _, _, ledger) = create_default_buffer(data_dir.clone()).await;
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
                .write_record(SizedRecord(64))
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

            let target_pos = expected_data_file_len as u64 - 8;
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
            let (mut writer, _, _, ledger) =
                create_default_buffer::<_, SizedRecord>(data_dir).await;
            marked_for_skip.assert();
            assert_reader_writer_file_positions!(ledger, 0, starting_writer_file_id);
            assert_file_does_not_exist_async!(&expected_final_write_data_file);

            // Do a simple write to ensure it opens the next data file.
            let _bytes_written = writer
                .write_record(SizedRecord(64))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");
            assert_reader_writer_file_positions!(ledger, 0, expected_final_writer_file_id);
            assert_file_exists_async!(&expected_final_write_data_file);
        }
    });

    let parent = trace_span!("writer_detects_when_last_record_has_scrambled_archive_data");
    fut.instrument(parent).await;
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
            let (mut writer, _, _, ledger) = create_default_buffer(data_dir.clone()).await;
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
                .write_record(SizedRecord(13))
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
            let (mut writer, _, _, ledger) =
                create_default_buffer::<_, SizedRecord>(data_dir).await;
            marked_for_skip.assert();
            assert_reader_writer_file_positions!(ledger, 0, starting_writer_file_id);
            assert_file_does_not_exist_async!(&expected_final_write_data_file);

            // Do a simple write to ensure it opens the next data file.
            let _bytes_written = writer
                .write_record(SizedRecord(64))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");
            assert_reader_writer_file_positions!(ledger, 0, expected_final_writer_file_id);
            assert_file_exists_async!(&expected_final_write_data_file);
        }
    });

    let parent = trace_span!("writer_detects_when_last_record_has_invalid_checksum");
    fut.instrument(parent).await;
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
            let (mut writer, _, _, ledger) = create_default_buffer(data_dir.clone()).await;
            let starting_writer_file_id = ledger.get_current_writer_file_id();
            let expected_final_writer_file_id = ledger.get_next_writer_file_id();
            let expected_final_write_data_file = ledger.get_next_writer_data_file_path();
            assert_file_does_not_exist_async!(&expected_final_write_data_file);

            // Write a regular record so something is in the data file.
            let bytes_written = writer
                .write_record(SizedRecord(64))
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
            let (mut writer, _, _, ledger) =
                create_default_buffer::<_, SizedRecord>(data_dir).await;
            marked_for_skip.assert();
            assert_reader_writer_file_positions!(ledger, 0, starting_writer_file_id);
            assert_file_does_not_exist_async!(&expected_final_write_data_file);

            // Do a simple write to ensure it opens the next data file.
            let _bytes_written = writer
                .write_record(SizedRecord(64))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");
            assert_reader_writer_file_positions!(ledger, 0, expected_final_writer_file_id);
            assert_file_exists_async!(&expected_final_write_data_file);
        }
    });

    let parent = trace_span!("writer_detects_when_last_record_wasnt_flushed");
    fut.instrument(parent).await;
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
            let (mut writer, _, _, ledger) = create_default_buffer(data_dir.clone()).await;
            let starting_writer_next_record_id = ledger.state().get_next_writer_record_id();
            let expected_final_writer_file_id = ledger.get_current_writer_file_id();
            let expected_final_write_data_file = ledger.get_next_writer_data_file_path();
            assert_file_does_not_exist_async!(&expected_final_write_data_file);

            // Write a regular record so something is in the data file.
            let bytes_written = writer
                .write_record(SizedRecord(64))
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
            let (_, _, _, ledger) = create_default_buffer::<_, SizedRecord>(data_dir).await;
            writer_did_not_call_reset.assert();
            assert_reader_writer_file_positions!(ledger, 0, expected_final_writer_file_id);
            assert_file_does_not_exist_async!(&expected_final_write_data_file);
            assert_eq!(
                actual_writer_next_record_id,
                ledger.state().get_next_writer_record_id()
            );
        }
    });

    let parent =
        trace_span!("writer_detects_when_last_record_was_flushed_but_id_wasnt_incremented");
    fut.instrument(parent).await;
}

#[tokio::test]
async fn reader_throws_error_when_record_is_undecodable_via_metadata() {
    static GET_METADATA_VALUE: AtomicU32 = AtomicU32::new(0);
    static CAN_DECODE_VALUE: AtomicU32 = AtomicU32::new(0);

    impl AsMetadata for u32 {
        fn into_u32(self) -> u32 {
            self
        }

        fn from_u32(value: u32) -> Option<Self> {
            if value < 32 {
                Some(value)
            } else {
                None
            }
        }
    }

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

    impl ByteSizeOf for ControllableRecord {
        fn allocated_bytes(&self) -> usize {
            0
        }
    }

    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, mut reader, _acker, _ledger) = create_default_buffer(data_dir).await;

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
            let second_read_error_reason = if let ReaderError::Incompatible { reason } = second_read_result.unwrap_err() {
                reason
            } else {
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
            let third_read_error_reason = if let ReaderError::Incompatible { reason } = third_read_result.unwrap_err() {
                reason
            } else {
                panic!("error should be ReadError::Incompatible");
            };
            let expected_third_read_error_reason_prefix = "invalid metadata for";
            assert!(third_read_error_reason.starts_with(expected_third_read_error_reason_prefix),
                "error reason when metadata cannot be converted should start with 'metadata invalid for', got '{}' instead",
                third_read_error_reason);
        }
    })
    .await;
}
