use async_trait::async_trait;
use criterion::{criterion_group, BatchSize, Criterion, SamplingMode, Throughput};
use futures::{
    stream::{self, BoxStream},
    Sink, SinkExt, Stream, StreamExt,
};
use tempfile::tempdir;
use tokio_stream::wrappers::ReceiverStream;
use vector::{
    buffers::{
        disk::{leveldb_buffer, DiskBuffer},
        Acker,
    },
    event::Event,
    sinks::util::StreamSink,
    test_util::{random_lines, runtime},
};

struct NullSink {
    acker: Acker,
}

impl NullSink {
    fn new(acker: Acker) -> Self {
        Self { acker }
    }
}

#[async_trait]
impl StreamSink for NullSink {
    async fn run(&mut self, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input.for_each(|_| async { self.acker.ack(1) }).await;
        Ok(())
    }
}

fn benchmark_buffers(c: &mut Criterion) {
    let num_lines: usize = 10_000;
    let line_size: usize = 200;

    let mut group = c.benchmark_group("isolated_buffers");
    group.throughput(Throughput::Bytes((num_lines * line_size) as u64));
    group.sampling_mode(SamplingMode::Flat);

    group.bench_function("channels/futures", |b| {
        b.iter_batched(
            || {
                let rt = runtime();

                let (writer, mut reader) = futures::channel::mpsc::channel(100);

                let read_handle = rt.spawn(async move { while reader.next().await.is_some() {} });

                (rt, writer, read_handle)
            },
            |(rt, writer, read_handle)| {
                let write_handle = rt.spawn(send_random(line_size, num_lines, writer));

                rt.block_on(write_handle).unwrap();
                rt.block_on(read_handle).unwrap();
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("channels/tokio", |b| {
        b.iter_batched(
            || {
                let rt = runtime();

                let (writer, reader) = tokio::sync::mpsc::channel(100);
                let mut stream = ReceiverStream::new(reader);

                let read_handle = rt.spawn(async move { while stream.next().await.is_some() {} });

                (rt, writer, read_handle)
            },
            |(rt, writer, read_handle)| {
                let write_handle = rt.spawn(async move {
                    let mut stream = random_events(line_size).take(num_lines);
                    while let Some(e) = stream.next().await {
                        writer.send(e).await.unwrap();
                    }
                });

                rt.block_on(write_handle).unwrap();
                rt.block_on(read_handle).unwrap();
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("leveldb/writing", |b| {
        b.iter_batched(
            || {
                let data_dir = tempdir().unwrap();

                let rt = runtime();

                let plenty_of_room = num_lines * line_size * 2;
                let (writer, _reader, _acker) =
                    leveldb_buffer::Buffer::build(data_dir.path().to_path_buf(), plenty_of_room)
                        .unwrap();

                (rt, writer)
            },
            |(rt, writer)| {
                let write_handle = rt.spawn(send_random(line_size, num_lines, writer));
                let _ = rt.block_on(write_handle).unwrap();
            },
            BatchSize::LargeInput,
        );
    });

    group.bench_function("leveldb/reading", |b| {
        b.iter_batched(
            || {
                let data_dir = tempdir().unwrap();

                let rt = runtime();

                let plenty_of_room = num_lines * line_size * 2;
                let (writer, reader, acker) =
                    leveldb_buffer::Buffer::build(data_dir.path().to_path_buf(), plenty_of_room)
                        .unwrap();

                let write_handle = rt.spawn(send_random(line_size, num_lines, writer));
                rt.block_on(write_handle).unwrap();

                let read_loop = async move { NullSink::new(acker).run(Box::pin(reader)).await };

                (rt, read_loop)
            },
            |(rt, read_loop)| {
                let read_handle = rt.spawn(read_loop);
                rt.block_on(read_handle).unwrap().unwrap();
            },
            BatchSize::LargeInput,
        );
    });

    group.bench_function("leveldb/both", |b| {
        b.iter_batched(
            || {
                let data_dir = tempdir().unwrap();

                let rt = runtime();

                let plenty_of_room = num_lines * line_size * 2;
                let (writer, reader, acker) =
                    leveldb_buffer::Buffer::build(data_dir.path().to_path_buf(), plenty_of_room)
                        .unwrap();

                let read_loop = async move { NullSink::new(acker).run(Box::pin(reader)).await };

                (rt, writer, read_loop)
            },
            |(rt, writer, read_loop)| {
                let read_handle = rt.spawn(read_loop);
                let write_handle = rt.spawn(send_random(line_size, num_lines, writer));

                let _ = rt.block_on(write_handle).unwrap();
                rt.block_on(read_handle).unwrap().unwrap();
            },
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

async fn send_random<E: std::fmt::Debug>(
    line_size: usize,
    num_lines: usize,
    mut writer: impl Sink<Event, Error = E> + Unpin,
) {
    let mut stream = random_events(line_size).map(Ok).take(num_lines);
    writer.send_all(&mut stream).await.unwrap();
}

fn random_events(size: usize) -> impl Stream<Item = Event> {
    stream::iter(random_lines(size)).map(Event::from)
}

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/timberio/vector/issues/5394
    config = Criterion::default().noise_threshold(0.05);
    targets = benchmark_buffers
);
