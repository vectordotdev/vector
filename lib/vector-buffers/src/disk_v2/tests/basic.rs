use tokio_test::{assert_pending, assert_ready, task::spawn};
use tracing::Instrument;

use super::{create_default_buffer, install_tracing_helpers, with_temp_dir, SizedRecord};
use crate::{assert_buffer_is_empty, assert_buffer_records};

#[tokio::test]
async fn basic_read_write_loop() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, mut reader, acker, ledger) = create_default_buffer(data_dir).await;
            assert_buffer_is_empty!(ledger);

            let expected_items = (512..768)
                .into_iter()
                .cycle()
                .take(2000)
                .map(SizedRecord)
                .collect::<Vec<_>>();
            let input_items = expected_items.clone();

            // Now create a reader and writer task that will take a set of input messages, buffer
            // them, read them out, and then make sure nothing was missed.
            let write_task = tokio::spawn(async move {
                for item in input_items {
                    writer
                        .write_record(item)
                        .await
                        .expect("write should not fail");
                }
                writer.flush().await.expect("writer flush should not fail");
                writer.close();
            });

            let read_task = tokio::spawn(async move {
                let mut items = Vec::new();
                while let Some(record) = reader.next().await.expect("reader should not fail") {
                    items.push(record);
                    acker.ack(1);
                }
                items
            });

            // Wait for both tasks to complete.
            write_task.await.expect("write task should not panic");
            let actual_items = read_task.await.expect("read task should not panic");

            // All records should be consumed at this point.
            assert_buffer_is_empty!(ledger);

            // Make sure we got the right items.
            assert_eq!(actual_items, expected_items);
        }
    })
    .await;
}

#[tokio::test]
async fn reader_exits_cleanly_when_writer_done_and_in_flight_acks() {
    let assertion_registry = install_tracing_helpers();

    let fut = with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a regular buffer, no customizations required.
            let (mut writer, mut reader, acker, ledger) = create_default_buffer(data_dir).await;
            assert_buffer_is_empty!(ledger);

            // Now write a single value and close the writer.
            writer
                .write_record(SizedRecord(32))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("writer flush should not fail");
            writer.close();
            assert_buffer_records!(ledger, 1);

            // And read that single value.
            let first_read = reader.next().await.expect("read should not fail");
            assert_eq!(first_read, Some(SizedRecord(32)));
            assert_buffer_records!(ledger, 1);

            debug!("MARK MARK MARK");

            // Now, we haven't acknowledged that read yet, so our next read should see the writer as
            // done but the total buffer size as >= 0, which means it has to wait for something,
            // which in this case is going to be the wakeup after we acknowledge the read.
            //
            // We have to poll until we hit the `wait_for_writer` call because there might be
            // wake-ups in between due to the actual file I/O calls we do in the `next` call waking
            // us up.
            //
            // Why do we need to make sure we enter `wait_for_writer` twice?  When the writer
            // closes, it always sends a wakeup in case there's a reader that's been (correctly)
            // waiting for new data.  When we do our first read here, there's data to be read, so
            // the reader never has to actually wait for the writer to make progress because it
            // doesn't yet think it's out of data.
            //
            // When we do the first poll of the second read, we will call `wait_for_writer` which
            // has a stored wakeup, which will let it proceed with another loop iteration, landing
            // it at the second call which is the one that causes it to actually block.
            let waiting_for_writer = assertion_registry
                .build()
                .with_name("wait_for_writer")
                .with_parent_name("reader_exits_cleanly_when_writer_done_and_in_flight_acks")
                .was_entered_at_least(2)
                .finalize();
            let mut blocked_read = spawn(async move { reader.next().await });
            while !waiting_for_writer.try_assert() {
                assert_pending!(blocked_read.poll());
            }

            // Now acknowledge the first read, which should wake up our blocked read.
            acker.ack(1);

            // Our blocked read should be woken up, and when we poll it, it should be also be ready,
            // albeit with a return value of `None`... because the writer is closed, and we read all
            // the records, so nothing is left. :)
            assert!(blocked_read.is_woken());
            let second_read = assert_ready!(blocked_read.poll());
            assert_eq!(second_read.expect("read should not fail"), None);

            // All records should be consumed at this point.
            assert_buffer_is_empty!(ledger);
        }
    });

    let parent = trace_span!("reader_exits_cleanly_when_writer_done_and_in_flight_acks");
    fut.instrument(parent).await;
}
