use futures::{SinkExt, StreamExt};
use tokio_test::{assert_pending, task::spawn};
use tracing::Instrument;

use super::create_default_buffer_v1;
use crate::{
    assert_reader_v1_delete_position, assert_reader_writer_v1_positions,
    test::common::{
        install_tracing_helpers, with_temp_dir, MultiEventRecord, PoisonPillMultiEventRecord,
        SizedRecord,
    },
    variants::disk_v1::{reader::FLUSH_INTERVAL, tests::drive_reader_to_flush},
    EventCount,
};

#[tokio::test]
async fn acking_single_event_advances_delete_offset() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, mut reader, acker) = create_default_buffer_v1(data_dir);
            assert_reader_writer_v1_positions!(reader, writer, 0, 0);

            // Write a simple single-event record and make writer offset moves forward by the
            // expected amount, since the entry key should be increment by event count:
            let record = SizedRecord(360);
            assert_eq!(record.event_count(), 1);
            writer
                .send(record.clone())
                .await
                .expect("write should not fail");
            assert_reader_writer_v1_positions!(reader, writer, 0, record.event_count());

            // And now read it out which should give us a matching record, while our delete offset
            // is still lagging behind the read offset since we haven't yet acknowledged the record:
            let read_record = reader.next().await.expect("read should not fail");
            assert_reader_writer_v1_positions!(
                reader,
                writer,
                record.event_count(),
                record.event_count()
            );

            assert_eq!(record, read_record);

            // Now acknowledge the record by using an amount equal to the record's event count, which should
            // be one but we're just trying to exercise the codepaths to make sure single-event records
            // work the same as multi-event records.
            //
            // Since the logic to acknowledge records is driven by trying to read, we have to
            // initiate a read first and then do our checks, but it also has to be fake spawned
            // since there's nothing else to read and we'd be awaiting forever.
            //
            // Additionally -- I know, I know -- we have to advance time to clear the flush
            // interval, since we only flush after a certain amount of time has elapsed to batch
            // deletes to the database:
            assert_reader_v1_delete_position!(reader, 0);
            assert_eq!(read_record.event_count(), 1);
            acker.ack(record.event_count());

            tokio::time::pause();

            let mut staged_read = spawn(reader.next());
            assert_pending!(staged_read.poll());
            drop(staged_read);

            assert_reader_v1_delete_position!(reader, 0);

            tokio::time::advance(FLUSH_INTERVAL).await;

            let mut staged_read = spawn(reader.next());
            assert_pending!(staged_read.poll());
            drop(staged_read);

            assert_reader_v1_delete_position!(reader, record.event_count());
        }
    })
    .await;
}

#[tokio::test]
async fn acking_multi_event_advances_delete_offset() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, mut reader, acker) = create_default_buffer_v1(data_dir);
            assert_reader_writer_v1_positions!(reader, writer, 0, 0);

            // Write a simple multi-event record and make writer offset moves forward by the
            // expected amount, since the entry key should be increment by event count:
            let record = MultiEventRecord(14);
            assert_eq!(record.event_count(), 14);
            writer
                .send(record.clone())
                .await
                .expect("write should not fail");
            assert_reader_writer_v1_positions!(reader, writer, 0, record.event_count());

            // And now read it out which should give us a matching record, while our delete offset
            // is still lagging behind the read offset since we haven't yet acknowledged the record:
            let read_record = reader.next().await.expect("read should not fail");
            assert_reader_writer_v1_positions!(
                reader,
                writer,
                record.event_count(),
                record.event_count()
            );

            assert_eq!(record, read_record);

            // Now acknowledge the record by using an amount equal to the record's event count.
            //
            // Since the logic to acknowledge records is driven by trying to read, we have to
            // initiate a read first and then do our checks, but it also has to be fake spawned
            // since there's nothing else to read and we'd be awaiting forever.
            //
            // Additionally -- I know, I know -- we have to advance time to clear the flush
            // interval, since we only flush after a certain amount of time has elapsed to batch
            // deletes to the database:
            assert_reader_v1_delete_position!(reader, 0);
            assert_eq!(read_record.event_count(), 14);
            acker.ack(record.event_count());

            tokio::time::pause();

            let mut staged_read = spawn(reader.next());
            assert_pending!(staged_read.poll());
            drop(staged_read);

            assert_reader_v1_delete_position!(reader, 0);

            tokio::time::advance(FLUSH_INTERVAL).await;

            let mut staged_read = spawn(reader.next());
            assert_pending!(staged_read.poll());
            drop(staged_read);

            assert_reader_v1_delete_position!(reader, record.event_count());
        }
    })
    .await;
}

#[tokio::test]
async fn acking_multi_event_advances_delete_offset_incremental() {
    let _a = install_tracing_helpers();
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, mut reader, acker) = create_default_buffer_v1(data_dir);
            assert_reader_writer_v1_positions!(reader, writer, 0, 0);

            // Write a simple multi-event record and make writer offset moves forward by the
            // expected amount, since the entry key should be increment by event count:
            let record = MultiEventRecord(14);
            assert_eq!(record.event_count(), 14);
            writer
                .send(record.clone())
                .await
                .expect("write should not fail");
            assert_reader_writer_v1_positions!(reader, writer, 0, record.event_count());

            // And now read it out which should give us a matching record, while our delete offset
            // is still lagging behind the read offset since we haven't yet acknowledged the record:
            let read_record = reader.next().await.expect("read should not fail");
            assert_reader_writer_v1_positions!(
                reader,
                writer,
                record.event_count(),
                record.event_count()
            );

            assert_eq!(record, read_record);

            // Now ack the record by using an amount equal to the record's event count, but do it
            // incrementally.
            //
            // Since the logic to acknowledge records is driven by trying to read, we have to
            // initiate a read first and then do our checks, but it also has to be fake spawned
            // since there's nothing else to read and we'd be awaiting forever.
            //
            // Additionally -- I know, I know -- we have to advance time to clear the flush
            // interval, since we only flush after a certain amount of time has elapsed to batch
            // deletes to the database:
            assert_reader_v1_delete_position!(reader, 0);
            assert_eq!(read_record.event_count(), 14);

            // Make sure our increments don't exceed the actual event count for the record:
            let increments = [4, 7, 2, 1];
            assert_eq!(read_record.event_count(), increments.iter().sum::<usize>());

            tokio::time::pause();

            // We expect the first three acknowledgements to do nothing, because the record will
            // still not have been fully acknowledged yet, but the fourth acknowledgement will make
            // the record eligible for deletion which should be reflected immediately:
            let expected_delete_pos = [0, 0, 0, record.event_count()];
            for (increment, expected) in increments.into_iter().zip(expected_delete_pos.into_iter())
            {
                acker.ack(increment);
                drive_reader_to_flush(&mut reader).await;

                assert_reader_v1_delete_position!(reader, expected);
            }
        }
    })
    .await;
}

#[tokio::test]
async fn acking_when_undecodable_records_present() {
    let _a = install_tracing_helpers();

    // We use a special message type, `PoisonPillMultiEventRecord`, which acts like a normal
    // multi-event record until you specify a special value for the event count which activates the
    // poison pill functionality.
    //
    // This lets us allow all records to be written to the buffer, but ensure that some of them will
    // fail to decode, which drives the logic that does deferred acknowledgements, gap/lost record
    // detection, etc.
    let poisoned_record = PoisonPillMultiEventRecord::poisoned();
    let poisoned_event_count = poisoned_record.event_count();
    let valid_record = PoisonPillMultiEventRecord(13);
    let valid_event_count = valid_record.event_count();
    let cases = vec![
        // A single poisoned record cannot be read, and without a follow up record, its event count
        // can also not be determined.
        (vec![poisoned_record.clone()], 0, 0),
        // A poisoned record that is followed by a valid record will ensure we can at least detect
        // how many events the poisoned record represented, even if we cannot read it and decode it.
        (
            vec![poisoned_record.clone(), valid_record.clone()],
            1,
            poisoned_event_count + valid_event_count,
        ),
        // We can also detect the event count of a poisoned record when it's followed by another
        // poisoned record, although neither of them will be able to be decoded and returned.
        (
            vec![poisoned_record.clone(), poisoned_record.clone()],
            0,
            poisoned_event_count,
        ),
    ];

    for (inputs, expected_reads, expected_delete_offset) in cases {
        // Make sure our test case parameters are valid:
        // - can't have have more expected reads than non-poisoned inputs
        assert!(expected_reads <= inputs.len());

        let fut = with_temp_dir(|dir| {
            let data_dir = dir.to_path_buf();
            let assertion_registry = install_tracing_helpers();

            async move {
                // Create a regular buffer, no customizations required.
                let (mut writer, mut reader, acker) = create_default_buffer_v1(data_dir);
                assert_reader_writer_v1_positions!(reader, writer, 0, 0);

                // Write all of our input records to the buffer, and make sure the sum of their
                // event count is represented correctly in the writer offset:
                let mut expected_writer_offset = 0;
                let num_writes = inputs.len();

                for input in inputs {
                    expected_writer_offset += input.event_count();
                    writer.send(input).await.expect("write should not fail");
                }

                assert_reader_writer_v1_positions!(reader, writer, 0, expected_writer_offset);

                // Track how many times we actually try to decode a buffered record, which should
                // always be at least as many times as we wrote a record, to ensure we actually read
                // all of the records back out, whether they're valid or invalid.
                let read_all_records = assertion_registry
                    .build()
                    .with_name("decode_next_record")
                    .with_parent_name("acking_when_undecodable_records_present")
                    .was_closed_at_least(num_writes)
                    .finalize();

                // Now attempt to read the expected number of valid records.  This means we will
                // expect these to come through, and thus wait for them:
                let mut remaining_reads = expected_reads;
                while remaining_reads > 0 {
                    let record = reader.next().await.expect("read should not fail");
                    acker.ack(record.event_count());

                    remaining_reads -= 1;
                }

                // Now drive reads against the reader until we've at least read all input records,
                // to ensure all data has been processed and accounted for before our final read
                // which ensures that all flushing has been handled and is quiesced:
                let mut staged_read = spawn(reader.next());
                while !read_all_records.try_assert() {
                    assert_pending!(staged_read.poll());
                }
                drop(staged_read);

                // Now we do one final staged read to drive the reader forward in terms of
                // acknowledgements.  We should essentially be able to correctly acknowledge any
                // records for which we've acknowledged fully or have enough data to otherwise
                // acknowledge:
                tokio::time::pause();
                drive_reader_to_flush(&mut reader).await;

                // Now make sure our delete offset is where we expect it to be:
                assert_reader_v1_delete_position!(reader, expected_delete_offset);
            }
        });

        let parent = trace_span!("acking_when_undecodable_records_present");
        fut.instrument(parent.or_current()).await;

        tokio::time::resume();
    }
}
