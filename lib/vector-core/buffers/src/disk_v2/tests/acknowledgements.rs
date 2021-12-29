use std::sync::Arc;

use tokio_test::{assert_pending, assert_ready, task::spawn};

use super::with_temp_dir;
use crate::{
    buffer_usage_data::BufferUsageHandle,
    disk_v2::{acknowledgements::create_disk_v2_acker, ledger::Ledger, DiskBufferConfig},
};

#[tokio::test]
async fn ack_updates_ledger_correctly() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            // Create a standalone ledger.
            let usage_handle = BufferUsageHandle::noop();
            let config = DiskBufferConfig::from_path(data_dir).build();
            let ledger = Ledger::load_or_create(config, usage_handle)
                .await
                .expect("ledger should not fail to load/create");
            assert_eq!(ledger.consume_pending_acks(), 0);

            // Create our acker and make sure it's empty.
            let ledger = Arc::new(ledger);
            let acker = create_disk_v2_acker(Arc::clone(&ledger));
            assert_eq!(ledger.consume_pending_acks(), 0);

            // Now make sure it updates pending acks.
            acker.ack(42);
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
            let config = DiskBufferConfig::from_path(data_dir).build();
            let ledger = Ledger::load_or_create(config, usage_handle)
                .await
                .expect("ledger should not fail to load/create");

            // Create our acker, as well as a future for awaiting writer progress, and make sure
            // it's not yet woken up.
            let ledger = Arc::new(ledger);
            let acker = create_disk_v2_acker(Arc::clone(&ledger));

            let mut wait_for_writer = spawn(ledger.wait_for_writer());
            assert_pending!(wait_for_writer.poll());
            assert!(!wait_for_writer.is_woken());

            // Now fire off an acknowledgement, and make sure our call woke up and can complete.
            acker.ack(314);
            assert!(wait_for_writer.is_woken());
            assert_ready!(wait_for_writer.poll());
        }
    })
    .await;
}
