use std::time::Duration;

use tokio::time::sleep;
use vector_common::finalization::Finalizable;

use super::create_default_buffer_v2;
use crate::test::{SizedRecord, acknowledge, install_tracing_helpers, with_temp_dir};

#[tokio::test]
async fn reader_receives_record_after_cold_start_write() {
    let _assertions = install_tracing_helpers();

    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let (mut writer, mut reader, _ledger) =
                create_default_buffer_v2::<_, SizedRecord>(data_dir).await;

            let reader_task = tokio::spawn(async move {
                let mut record = reader
                    .next()
                    .await
                    .expect("read should not fail")
                    .expect("should get a record, not EOF");
                acknowledge(record.take_finalizers()).await;
                record
            });

            // Leave the buffer genuinely idle before the first write, matching low-volume sources
            // where the reader is already waiting when the first record arrives.
            sleep(Duration::from_millis(50)).await;

            writer
                .write_record(SizedRecord::new(32))
                .await
                .expect("write should not fail");
            writer.flush().await.expect("flush should not fail");

            let record = tokio::time::timeout(Duration::from_secs(2), reader_task)
                .await
                .expect("reader should receive the record without another write")
                .expect("reader task should not panic");
            assert_eq!(record, SizedRecord::new(32));
        }
    })
    .await;
}
