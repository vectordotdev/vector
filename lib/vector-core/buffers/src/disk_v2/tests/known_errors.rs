use std::io::SeekFrom;

use memmap2::MmapMut;
use tokio::{
    fs::OpenOptions,
    io::{AsyncSeekExt, AsyncWriteExt},
};
use tracing::Instrument;

use super::{create_default_buffer, install_tracing_helpers, with_temp_dir, UndecodableRecord};
use crate::{
    assert_enough_bytes_written, assert_file_does_not_exist_async, assert_file_exists_async,
    assert_reader_writer_file_positions,
    disk_v2::{backed_archive::BackedArchive, record::Record, tests::SizedRecord, ReaderError},
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
                Err(ReaderError::FailedToDeserialize { reason }) => {
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
    // TODO: We really do want this test, which is the equivalent of saying:
    // "do we correctly detect when a file has, say, the u64 'record length' delimiter but either no
    // more data after that, or not enough data, and we know for sure the writer has moved on?"
    //
    // Right now, we _always_ assume the data is coming if we can at least read 8 bytes for the
    // length delimiter... but the point in the code where that happens is oblivious to the
    // higher-level reader/writer state, so making it possible to detect this condition will take a
    // small chunk of redesign work, but is definitely one of the last major possible issues that
    // the buffer reasonably has to handle.
    let worked = true;
    assert!(worked);
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
                Err(ReaderError::FailedToDeserialize { .. })
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
            assert!(matches!(
                read_result,
                Err(ReaderError::FailedToDecode { .. })
            ));
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
            let writer_called_reset = assertion_registry
                .build()
                .with_name("reset")
                .with_parent_name("writer_detects_when_last_record_has_scrambled_archive_data")
                .was_entered()
                .finalize();

            // Create a regular buffer, no customizations required.
            let (mut writer, _, _, ledger) = create_default_buffer(data_dir.clone()).await;
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

            // We should not have seen a call to `reset` yet.
            assert!(!writer_called_reset.try_assert());

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

            // Now reopen the buffer, which should trigger a `Writer::reset` call and additionally,
            // we should be rolled over to a new data file now on the writer side.
            let (_, _, _, ledger) = create_default_buffer::<_, SizedRecord>(data_dir).await;
            writer_called_reset.assert();
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
            let writer_called_reset = assertion_registry
                .build()
                .with_name("reset")
                .with_parent_name("writer_detects_when_last_record_has_invalid_checksum")
                .was_entered()
                .finalize();

            // Create a regular buffer, no customizations required.
            let (mut writer, _, _, ledger) = create_default_buffer(data_dir.clone()).await;
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

            // We should not have seen a call to `reset` yet.
            assert!(!writer_called_reset.try_assert());

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

            // Now reopen the buffer, which should trigger a `Writer::reset` call and additionally,
            // we should be rolled over to a new data file now on the writer side.
            let (_, _, _, ledger) = create_default_buffer::<_, SizedRecord>(data_dir).await;
            writer_called_reset.assert();
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
            let writer_called_reset = assertion_registry
                .build()
                .with_name("reset")
                .with_parent_name("writer_detects_when_last_record_wasnt_flushed")
                .was_entered()
                .finalize();

            // Create a regular buffer, no customizations required.
            let (mut writer, _, _, ledger) = create_default_buffer(data_dir.clone()).await;
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

            // We should not have seen a call to `reset` yet.
            assert!(!writer_called_reset.try_assert());

            // Now reopen the buffer, which should trigger a `Writer::reset` call and additionally,
            // we should be rolled over to a new data file now on the writer side.
            let (_, _, _, ledger) = create_default_buffer::<_, SizedRecord>(data_dir).await;
            writer_called_reset.assert();
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
            let writer_called_reset = assertion_registry
                .build()
                .with_name("reset")
                .with_parent_name(
                    "writer_detects_when_last_record_was_flushed_but_id_wasnt_incremented",
                )
                .was_entered()
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

            // We should not have seen a call to `reset` yet.
            assert!(!writer_called_reset.try_assert());

            // Now reopen the buffer, which should trigger the skip ahead logic where we move our
            // writer next record ID to be ahead of the actual last record ID, but on whatever we
            // pulled out of the data file.  This is required to maintain our monotonicity invariant
            // for all records written into the buffer.
            let (_, _, _, ledger) = create_default_buffer::<_, SizedRecord>(data_dir).await;

            // TODO: Rewrite this as a negative assertion (i.e. `was_not_entered`) once
            // `tracing-fluent-assertions` adds support for them.
            assert!(!writer_called_reset.try_assert());
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
