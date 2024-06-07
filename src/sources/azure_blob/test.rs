use super::*;
use crate::{
    config::LogNamespace, event::EventStatus, serde::default_decoding, shutdown::ShutdownSignal,
    test_util::collect_n, SourceSender,
};
use tokio::{select, sync::oneshot};

#[tokio::test]
async fn test_messages_delivered() {
    let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);
    let streamer = super::AzureBlobStreamer::new(
        ShutdownSignal::noop(),
        tx,
        LogNamespace::Vector,
        true,
        default_decoding(),
    );
    let mut streamer = streamer.expect("Failed to create streamer");
    let (success_sender, success_receiver) = oneshot::channel();
    let blob_pack = BlobPack {
        row_stream: Box::pin(stream! {
            let lines = vec!["foo", "bar"];
            for line in lines {
                yield line.as_bytes().to_vec();
            }
        }),
        success_handler: Box::new(move || {
            Box::pin(async move {
                success_sender.send(()).unwrap();
            })
        }),
    };
    let (events_collector, events_receiver) = oneshot::channel();
    tokio::spawn(async move {
        events_collector.send(collect_n(rx, 2).await).unwrap();
    });
    streamer
        .process_blob_pack(blob_pack)
        .await
        .expect("Failed processing blob pack");

    let events = select! {
        value = events_receiver => value.expect("Failed to receive events"),
        _ = time::sleep(Duration::from_secs(5)) => panic!("Timeout waiting for events"),
    };
    assert_eq!(events[0].as_log().value().to_string(), "\"foo\"");
    assert_eq!(events[1].as_log().value().to_string(), "\"bar\"");
    select!{
        _ = success_receiver => {}
        _ = time::sleep(Duration::from_secs(5)) => panic!("Timeout waiting for success handler"),
    }
}

#[tokio::test]
async fn test_messages_rejected() {
    let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Rejected);
    let streamer = super::AzureBlobStreamer::new(
        ShutdownSignal::noop(),
        tx,
        LogNamespace::Vector,
        true,
        default_decoding(),
    );
    let mut streamer = streamer.expect("Failed to create streamer");
    let (success_sender, mut success_receiver) = oneshot::channel();
    let blob_pack = BlobPack {
        row_stream: Box::pin(stream! {
            let lines = vec!["foo", "bar"];
            for line in lines {
                yield line.as_bytes().to_vec();
            }
        }),
        success_handler: Box::new(move || {
            Box::pin(async move {
                success_sender.send(()).unwrap();
            })
        }),
    };
    let (events_collector, events_receiver) = oneshot::channel();
    tokio::spawn(async move {
        events_collector.send(collect_n(rx, 2).await).unwrap();
    });
    streamer
        .process_blob_pack(blob_pack)
        .await
        .expect("Failed processing blob pack");

    let events = select! {
        value = events_receiver => value.expect("Failed to receive events"),
        _ = time::sleep(Duration::from_secs(5)) => panic!("Timeout waiting for events"),
    };
    assert_eq!(events[0].as_log().value().to_string(), "\"foo\"");
    assert_eq!(events[1].as_log().value().to_string(), "\"bar\"");
    assert!(success_receiver.try_recv().is_err()); // assert
}
