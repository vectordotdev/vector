use criterion::{criterion_group, BatchSize, Criterion, SamplingMode, Throughput};
use futures::{
    compat::{Future01CompatExt, Stream01CompatExt},
    stream::StreamExt,
};
use futures01::{stream, AsyncSink, Poll, Sink, StartSend, Stream};
use tempfile::tempdir;
use vector::{
    buffers::disk::{leveldb_buffer, DiskBuffer},
    sinks::util::StreamSinkOld,
    test_util::{random_lines, runtime},
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
    let num_lines: usize = 10_000;
    let line_size: usize = 200;

    let mut group = c.benchmark_group("isolated_buffers");
    group.throughput(Throughput::Bytes((num_lines * line_size) as u64));
    group.sampling_mode(SamplingMode::Flat);

    group.bench_function("channels/futures01", |b| {
        b.iter_batched(
            || {
                let rt = runtime();

                let (writer, reader) = futures01::sync::mpsc::channel(100);
                let writer = writer.sink_map_err(|e| panic!(e));

                let read_loop = reader.for_each(move |_| Ok(()));

                (rt, writer, read_loop)
            },
            |(mut rt, writer, read_loop)| {
                let send = writer.send_all(random_events(line_size).take(num_lines as u64));

                let read_handle = rt.spawn(read_loop.compat());
                let write_handle = rt.spawn(send.compat());

                let (writer, _stream) = rt.block_on(write_handle).unwrap().unwrap();
                drop(writer);

                rt.block_on(read_handle).unwrap().unwrap();
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("channels/tokio", |b| {
        b.iter_batched(
            || {
                let rt = runtime();

                let (writer, mut reader) = tokio::sync::mpsc::channel(100);

                let read_handle = rt.spawn(async move { while reader.next().await.is_some() {} });

                (rt, writer, read_handle)
            },
            |(mut rt, mut writer, read_handle)| {
                let write_handle = rt.spawn(async move {
                    let mut stream = random_events(line_size).take(num_lines as u64).compat();
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
            |(mut rt, writer)| {
                let send = writer.send_all(random_events(line_size).take(num_lines as u64));
                let write_handle = rt.spawn(send.compat());
                let _ = rt.block_on(write_handle).unwrap().unwrap();
            },
            BatchSize::LargeInput,
        );
    });

    group.bench_function("leveldb/reading", |b| {
        b.iter_batched(
            || {
                let data_dir = tempdir().unwrap();

                let mut rt = runtime();

                let plenty_of_room = num_lines * line_size * 2;
                let (writer, reader, acker) =
                    leveldb_buffer::Buffer::build(data_dir.path().to_path_buf(), plenty_of_room)
                        .unwrap();

                let send = writer.send_all(random_events(line_size).take(num_lines as u64));
                let write_handle = rt.spawn(send.compat());
                let (writer, _stream) = rt.block_on(write_handle).unwrap().unwrap();
                drop(writer);

                let read_loop = StreamSinkOld::new(NullSink, acker).send_all(reader);

                (rt, read_loop)
            },
            |(mut rt, read_loop)| {
                let read_handle = rt.spawn(read_loop.compat());
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

                let read_loop = StreamSinkOld::new(NullSink, acker).send_all(reader);

                (rt, writer, read_loop)
            },
            |(mut rt, writer, read_loop)| {
                let send = writer.send_all(random_events(line_size).take(num_lines as u64));

                let read_handle = rt.spawn(read_loop.compat());
                let write_handle = rt.spawn(send.compat());

                let _ = rt.block_on(write_handle).unwrap().unwrap();
                rt.block_on(read_handle).unwrap().unwrap();
            },
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

fn random_events(size: usize) -> impl Stream<Item = Event, Error = ()> {
    stream::iter_ok(random_lines(size)).map(Event::from)
}

criterion_group!(benches, benchmark_buffers);
