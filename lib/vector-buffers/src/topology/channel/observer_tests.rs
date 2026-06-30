use std::{
    num::{NonZeroU64, NonZeroUsize},
    path::Path,
};

use tracing::Span;
use vector_common::{byte_size_of::ByteSizeOf, finalization::Finalizable};

use crate::{
    BufferUsageObserver, Bufferable, WhenFull,
    config::MemoryBufferSize,
    test::{MultiEventRecord, SizedRecord, acknowledge, with_temp_dir},
    topology::{
        builder::TopologyBuilder,
        channel::{BufferReceiver, BufferSender},
    },
    variants::{DiskV2Buffer, MemoryBuffer},
};

type ObservedBuffer<T> = (BufferSender<T>, BufferReceiver<T>, BufferUsageObserver);

async fn build_memory_buffer_with_observer<T: Bufferable>(
    max_events: NonZeroUsize,
) -> ObservedBuffer<T> {
    let mut builder = TopologyBuilder::default();
    builder.stage(MemoryBuffer::with_max_events(max_events), WhenFull::Block);
    builder
        .build_with_observer(String::from("observer-test"), Span::none(), true)
        .await
        .expect("memory topology should build")
}

async fn build_byte_size_memory_buffer_with_observer<T: Bufferable>(
    max_bytes: NonZeroUsize,
) -> ObservedBuffer<T> {
    let mut builder = TopologyBuilder::default();
    builder.stage(
        MemoryBuffer::new(MemoryBufferSize::MaxSize(max_bytes)),
        WhenFull::Block,
    );
    builder
        .build_with_observer(String::from("observer-test"), Span::none(), true)
        .await
        .expect("byte-size memory topology should build")
}

async fn build_overflow_buffer_with_observer<T>() -> ObservedBuffer<T>
where
    T: Bufferable,
{
    let mut builder = TopologyBuilder::default();
    builder.stage(
        MemoryBuffer::with_max_events(NonZeroUsize::new(1).unwrap()),
        WhenFull::Overflow,
    );
    builder.stage(
        MemoryBuffer::with_max_events(NonZeroUsize::new(100).unwrap()),
        WhenFull::Block,
    );
    builder
        .build_with_observer(String::from("observer-test"), Span::none(), true)
        .await
        .expect("overflow topology should build")
}

async fn build_disk_buffer_with_observer_in<T>(dir: &Path) -> ObservedBuffer<T>
where
    T: Bufferable + Clone + Finalizable,
{
    let mut builder = TopologyBuilder::default();
    builder.stage(
        DiskV2Buffer::new(
            String::from("observer-test"),
            dir.to_path_buf(),
            NonZeroU64::new(300_000_000).unwrap(),
        ),
        WhenFull::Block,
    );
    builder
        .build_with_observer(String::from("observer-test"), Span::none(), true)
        .await
        .expect("disk topology should build")
}

async fn build_memory_buffer_with_observe<T: Bufferable>(observe: bool) -> ObservedBuffer<T> {
    let mut builder = TopologyBuilder::default();
    builder.stage(
        MemoryBuffer::with_max_events(NonZeroUsize::new(100).unwrap()),
        WhenFull::Block,
    );
    builder
        .build_with_observer(String::from("observer-test"), Span::none(), observe)
        .await
        .expect("memory topology should build")
}

#[tokio::test]
async fn observe_flag_controls_observer_received_accounting() {
    let (mut observed_tx, _observed_rx, observed) =
        build_memory_buffer_with_observe::<SizedRecord>(true).await;
    let observed_record = SizedRecord::new(7);
    let expected = observed_record.size_of() as u64;
    observed_tx.send(observed_record, None).await.unwrap();
    assert_eq!(observed.received(), (1, expected));

    let (mut unobserved_tx, _unobserved_rx, unobserved) =
        build_memory_buffer_with_observe::<SizedRecord>(false).await;
    unobserved_tx.send(SizedRecord::new(7), None).await.unwrap();
    assert_eq!(unobserved.received(), (0, 0));
}

#[tokio::test]
async fn observer_received_counts_size_of_once_on_memory() {
    let (mut tx, mut rx, observer) =
        build_memory_buffer_with_observer::<SizedRecord>(NonZeroUsize::new(100).unwrap()).await;
    let record = SizedRecord::new(7);
    let expected = record.size_of() as u64;

    tx.send(record, None).await.unwrap();

    assert_eq!(observer.received(), (1, expected));
    let _ = rx.next().await;
    assert_eq!(observer.received(), (1, expected));
}

#[tokio::test]
async fn observer_received_counts_size_of_once_on_disk() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let (mut tx, _rx, observer) =
                build_disk_buffer_with_observer_in::<SizedRecord>(&data_dir).await;
            let record = SizedRecord::new(7);
            let expected = record.size_of() as u64;

            tx.send(record, None).await.unwrap();

            assert_eq!(observer.received(), (1, expected));
        }
    })
    .await;
}

#[tokio::test]
async fn observer_received_not_double_counted_on_overflow() {
    let (mut tx, _rx, observer) = build_overflow_buffer_with_observer::<SizedRecord>().await;

    tx.send(SizedRecord::new(1), None).await.unwrap();
    tx.send(SizedRecord::new(1), None).await.unwrap();

    assert_eq!(observer.received().0, 2);
}

#[tokio::test]
async fn occupancy_tracks_event_count_on_memory() {
    let (mut tx, mut rx, observer) =
        build_memory_buffer_with_observer::<MultiEventRecord>(NonZeroUsize::new(100).unwrap())
            .await;

    tx.send(MultiEventRecord::new(3), None).await.unwrap();
    tx.send(MultiEventRecord::new(2), None).await.unwrap();
    assert_eq!(observer.occupancy().0, 5);

    let _ = rx.next().await;
    assert_eq!(observer.occupancy().0, 2);
}

#[tokio::test]
async fn occupancy_tracks_fill_on_disk() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            let (mut tx, mut rx, observer) =
                build_disk_buffer_with_observer_in::<SizedRecord>(&data_dir).await;

            tx.send(SizedRecord::new(1), None).await.unwrap();
            tx.flush().await.unwrap();
            assert!(observer.occupancy().1 > 0);

            let record = rx.next().await.unwrap();
            acknowledge(record).await;
            drop(tx);
            while rx.next().await.is_some() {}
            assert_eq!(observer.occupancy().1, 0);
        }
    })
    .await;
}

#[tokio::test]
async fn occupancy_seeded_after_restart_with_existing_data() {
    with_temp_dir(|dir| {
        let data_dir = dir.to_path_buf();

        async move {
            {
                let (mut tx, _rx, _observer) =
                    build_disk_buffer_with_observer_in::<SizedRecord>(&data_dir).await;
                tx.send(SizedRecord::new(1), None).await.unwrap();
                tx.flush().await.unwrap();
            }

            let (_tx, _rx, observer) =
                build_disk_buffer_with_observer_in::<SizedRecord>(&data_dir).await;
            assert!(observer.occupancy().1 > 0);
        }
    })
    .await;
}

#[tokio::test]
async fn max_size_reports_bytes_for_byte_capacity_buffer() {
    let cap = NonZeroUsize::new(1_000_000).unwrap();
    let (_tx, _rx, observer) =
        build_byte_size_memory_buffer_with_observer::<SizedRecord>(cap).await;

    assert_eq!(observer.max_size().1, 1_000_000);
}
