use std::sync::Arc;

use tokio_test::{assert_pending, assert_ready, task::spawn};
use vector_common::finalization::{BatchNotifier, EventFinalizer, EventStatus};

use crate::{
    buffer_usage_data::BufferUsageHandle,
    test::with_temp_dir,
    variants::disk_v2::{ledger::Ledger, DiskBufferConfigBuilder},
};

pub(crate) async fn acknowledge(batch: BatchNotifier) {
    let finalizer = EventFinalizer::new(batch);
    finalizer.update_status(EventStatus::Delivered);
    drop(finalizer); // This sends the status update
    tokio::task::yield_now().await;
}

#[tokio::test]
async fn ack_updates_ledger_correctly() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a standalone ledger.
            let usage_handle = BufferUsageHandle::noop();
            let config = DiskBufferConfigBuilder::from_path(data_dir)
                .build()
                .expect("creating buffer should not fail");
            let ledger = Ledger::load_or_create(config, usage_handle)
                .await
                .expect("ledger should not fail to load/create");
            assert_eq!(ledger.consume_pending_acks(), 0);

            // Create our ledger, and make sure it's empty.
            let ledger = Arc::new(ledger);
            let finalizer = Arc::clone(&ledger).spawn_finalizer();
            assert_eq!(ledger.consume_pending_acks(), 0);

            // Now make sure it updates pending acks.
            let (batch, receiver) = BatchNotifier::new_with_receiver();
            finalizer.add(42, receiver);
            acknowledge(batch).await;
            assert_eq!(ledger.consume_pending_acks(), 42);
            assert_eq!(ledger.consume_pending_acks(), 0);
        }
    })
    .await;
}

#[tokio::test]
async fn ack_wakes_reader() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a standalone ledger.
            let usage_handle = BufferUsageHandle::noop();
            let config = DiskBufferConfigBuilder::from_path(data_dir)
                .build()
                .expect("creating buffer should not fail");
            let ledger = Ledger::load_or_create(config, usage_handle)
                .await
                .expect("ledger should not fail to load/create");

            // Create our ledger, as well as a future for awaiting
            // writer progress, and make sure it's not yet woken up.
            let ledger = Arc::new(ledger);
            let finalizer = Arc::clone(&ledger).spawn_finalizer();

            let mut wait_for_writer = spawn(ledger.wait_for_writer());
            assert_pending!(wait_for_writer.poll());
            assert!(!wait_for_writer.is_woken());

            // Now fire off an acknowledgement, and make sure our call woke up and can complete.
            let (batch, receiver) = BatchNotifier::new_with_receiver();
            finalizer.add(1, receiver);
            acknowledge(batch).await;

            assert!(wait_for_writer.is_woken());
            assert_ready!(wait_for_writer.poll());
        }
    })
    .await;
}
