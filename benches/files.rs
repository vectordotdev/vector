use std::{convert::TryInto, path::PathBuf, time::Duration};

use bytes::Bytes;
use criterion::{criterion_group, BatchSize, Criterion, SamplingMode, Throughput};
use futures::{stream, SinkExt, StreamExt};
use tempfile::tempdir;
use tokio::fs::OpenOptions;
use tokio_util::codec::{BytesCodec, FramedWrite};
use vector::{
    config, sinks, sources,
    test_util::{random_lines, runtime, start_topology},
};
use vector_lib::codecs::{encoding::FramingConfig, TextSerializerConfig};

fn benchmark_files_no_partitions(c: &mut Criterion) {
    let num_lines: usize = 10_000;
    let line_size: usize = 100;

    let mut group = c.benchmark_group("files");
    group.throughput(Throughput::Bytes((num_lines * line_size) as u64));
    group.sampling_mode(SamplingMode::Flat);

    group.bench_function("no_partitions", |b| {
        b.iter_batched(
            || {
                let temp = tempdir().unwrap();
                let directory = temp.path().to_path_buf();

                let directory_str = directory.to_str().unwrap();

                let mut data_dir = directory_str.to_owned();
                data_dir.push_str("/data");
                let data_dir = PathBuf::from(data_dir);
                std::fs::remove_dir_all(&data_dir).unwrap_or(());
                std::fs::create_dir(&data_dir).unwrap_or(());
                //if it doesn't exist -- that's ok

                let mut input = directory_str.to_owned();
                input.push_str("/test.in");

                let input = PathBuf::from(input);

                let mut output = directory_str.to_owned();
                output.push_str("/test.out");

                let mut source = sources::file::FileConfig::default();
                source.include.push(input.clone());
                source.data_dir = Some(data_dir);

                let mut config = config::Config::builder();
                config.add_source("in", source);
                config.add_sink(
                    "out",
                    &["in"],
                    sinks::file::FileSinkConfig {
                        path: output.try_into().unwrap(),
                        idle_timeout: Duration::from_secs(30),
                        encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
                        compression: sinks::file::Compression::None,
                        acknowledgements: Default::default(),
                        timezone: Default::default(),
                        internal_metrics: Default::default(),
                    },
                );

                let rt = runtime();
                let (topology, input) = rt.block_on(async move {
                    let (topology, _crash) = start_topology(config.build().unwrap(), false).await;

                    let mut options = OpenOptions::new();
                    options.create(true).write(true);

                    let input = options.open(input).await.unwrap();

                    (topology, input)
                });
                (rt, topology, input)
            },
            |(rt, topology, input)| {
                rt.block_on(async move {
                    let mut sink = FramedWrite::new(input, BytesCodec::new());
                    let raw_lines = random_lines(line_size).take(num_lines).map(|mut line| {
                        line.push('\n');
                        Bytes::from(line)
                    });
                    let mut lines = stream::iter(raw_lines);
                    while let Some(line) = lines.next().await {
                        sink.send(line).await.unwrap();
                    }

                    topology.stop().await;
                });
            },
            BatchSize::LargeInput,
        )
    });

    group.finish();
}

criterion_group!(
    name = benches;
    // encapsulates inherent CI noise we saw in
    // https://github.com/vectordotdev/vector/issues/5394
    config = Criterion::default().noise_threshold(0.05);
    targets = benchmark_files_no_partitions
);
