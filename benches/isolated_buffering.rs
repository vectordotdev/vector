#![allow(clippy::redundant_pattern_matching)]

use criterion::{criterion_group, criterion_main, Benchmark, Criterion, Throughput};
use futures::{
    compat::{Future01CompatExt, Stream01CompatExt},
    stream::StreamExt,
};
use futures01::{stream, AsyncSink, Poll, Sink, StartSend, Stream};
use tempfile::tempdir;
use vector::{
    buffers::disk::{leveldb_buffer, DiskBuffer},
    runtime,
    sinks::util::StreamSink,
    Event,
};

struct NullSink;

impl Sink for NullSink {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(&mut self, _item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        Ok(().into())
    }
}

fn benchmark_buffers(c: &mut Criterion) {
    let num_lines: usize = 100_000;
    let line_size: usize = 200;

    let data_dir = tempdir().unwrap();
    let data_dir = data_dir.path().to_path_buf();
    let data_dir2 = data_dir.clone();
    let data_dir3 = data_dir.clone();

    c.bench(
        "buffers",
        Benchmark::new("channels/futures01", move |b| {
            b.iter_with_setup(
                || {
                    let rt = runtime::Runtime::new().unwrap();

                    let (writer, reader) = futures01::sync::mpsc::channel(100);
                    let writer = writer.sink_map_err(|e| panic!(e));

                    let read_loop = reader.for_each(move |_| Ok(()));

                    (rt, writer, read_loop)
                },
                |(mut rt, writer, read_loop)| {
                    let send = writer.send_all(random_events(line_size).take(num_lines as u64));

                    let read_handle = rt.spawn_handle_std(read_loop.compat());
                    let write_handle = rt.spawn_handle_std(send.compat());

                    let (writer, _stream) = rt.block_on_std(write_handle).unwrap().unwrap();
                    drop(writer);

                    rt.block_on_std(read_handle).unwrap().unwrap();
                },
            );
        })
        .with_function("channels/tokio", move |b| {
            b.iter_with_setup(
                || {
                    let mut rt = runtime::Runtime::new().unwrap();

                    let (writer, mut reader) = tokio::sync::mpsc::channel(100);

                    let read_handle =
                        rt.spawn_handle_std(
                            async move { while let Some(_) = reader.next().await {} },
                        );

                    (rt, writer, read_handle)
                },
                |(mut rt, mut writer, read_handle)| {
                    let write_handle = rt.spawn_handle_std(async move {
                        let mut stream = random_events(line_size).take(num_lines as u64).compat();
                        while let Some(e) = stream.next().await {
                            writer.send(e).await.unwrap();
                        }
                    });

                    rt.block_on_std(write_handle).unwrap();
                    rt.block_on_std(read_handle).unwrap();
                },
            );
        })
        .with_function("leveldb/writing", move |b| {
            b.iter_with_setup(
                || {
                    let rt = runtime::Runtime::new().unwrap();

                    let path = data_dir.join("basic_sink");

                    // Clear out any existing data
                    if let Ok(_) = std::fs::metadata(&path) {
                        std::fs::remove_dir_all(&path).unwrap();
                    }

                    let plenty_of_room = num_lines * line_size * 2;
                    let (writer, _reader, _acker) =
                        leveldb_buffer::Buffer::build(path, plenty_of_room).unwrap();

                    (rt, writer)
                },
                |(mut rt, writer)| {
                    let send = writer.send_all(random_events(line_size).take(num_lines as u64));
                    let write_handle = rt.spawn_handle_std(send.compat());
                    let _ = rt.block_on_std(write_handle).unwrap().unwrap();
                },
            );
        })
        .with_function("leveldb/reading", move |b| {
            b.iter_with_setup(
                || {
                    let mut rt = runtime::Runtime::new().unwrap();

                    let path = data_dir2.join("basic_sink");

                    // Clear out any existing data
                    if let Ok(_) = std::fs::metadata(&path) {
                        std::fs::remove_dir_all(&path).unwrap();
                    }

                    let plenty_of_room = num_lines * line_size * 2;
                    let (writer, reader, acker) =
                        leveldb_buffer::Buffer::build(path, plenty_of_room).unwrap();

                    let send = writer.send_all(random_events(line_size).take(num_lines as u64));
                    let write_handle = rt.spawn_handle_std(send.compat());
                    let (writer, _stream) = rt.block_on_std(write_handle).unwrap().unwrap();
                    drop(writer);

                    let read_loop = StreamSink::new(NullSink, acker).send_all(reader);

                    (rt, read_loop)
                },
                |(mut rt, read_loop)| {
                    let read_handle = rt.spawn_handle_std(read_loop.compat());
                    rt.block_on_std(read_handle).unwrap().unwrap();
                },
            );
        })
        .with_function("leveldb/both", move |b| {
            b.iter_with_setup(
                || {
                    let rt = runtime::Runtime::new().unwrap();

                    let path = data_dir3.join("basic_sink");

                    // Clear out any existing data
                    if let Ok(_) = std::fs::metadata(&path) {
                        std::fs::remove_dir_all(&path).unwrap();
                    }

                    let plenty_of_room = num_lines * line_size * 2;
                    let (writer, reader, acker) =
                        leveldb_buffer::Buffer::build(path, plenty_of_room).unwrap();

                    let read_loop = StreamSink::new(NullSink, acker).send_all(reader);

                    (rt, writer, read_loop)
                },
                |(mut rt, writer, read_loop)| {
                    let send = writer.send_all(random_events(line_size).take(num_lines as u64));

                    let read_handle = rt.spawn_handle_std(read_loop.compat());
                    let write_handle = rt.spawn_handle_std(send.compat());

                    let _ = rt.block_on_std(write_handle).unwrap().unwrap();
                    rt.block_on_std(read_handle).unwrap().unwrap();
                },
            );
        })
        .sample_size(10)
        .noise_threshold(0.05)
        .throughput(Throughput::Bytes((num_lines * line_size) as u64)),
    );
}

criterion_group!(buffers, benchmark_buffers);
criterion_main!(buffers);

fn random_events(size: usize) -> impl Stream<Item = Event, Error = ()> {
    use rand::distributions::Alphanumeric;
    use rand::{rngs::SmallRng, thread_rng, Rng, SeedableRng};

    let mut rng = SmallRng::from_rng(thread_rng()).unwrap();

    let lines = std::iter::repeat(()).map(move |_| {
        rng.sample_iter(&Alphanumeric)
            .take(size)
            .collect::<String>()
    });
    stream::iter_ok(lines).map(Event::from)
}
