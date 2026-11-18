//! Actual native adapter: serialization, metadata, finalizers, record construction, and drain.
use std::{marker::PhantomData, time::Duration};

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use futures::StreamExt;
use vector::{
    codecs::{Encoder, Transformer},
    event::{BatchNotifier, Event, LogEvent},
    sinks::{
        aws_kinesis::{
            aggregation::{Format, Settings},
            newline::requests,
            request_builder::KinesisRequestBuilder,
            streams::KinesisStreamRecord,
        },
        util::Compression,
    },
};
use vector_lib::{codecs::TextSerializerConfig, request_metadata::MetaDescriptive};

fn benchmark(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut group = c.benchmark_group("kinesis/native_adapter");
    group
        .sample_size(30)
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(5));
    let count = 5000_usize;
    group.throughput(Throughput::Elements(u64::try_from(count).unwrap()));
    for (format_name, format) in [("newline", Format::Newline), ("kpl", Format::Kpl)] {
        for size in [100, 1024, 10240] {
            let template = LogEvent::from("x".repeat(size));
            group.bench_function(format!("{format_name}_text_{size}B_with_finalizers"), |b| {
                b.iter_batched(
                    || {
                        let (batch, receiver) = BatchNotifier::new_with_receiver();
                        let events = (0..count)
                            .map(|_| Event::Log(template.clone().with_batch_notifier(&batch)))
                            .collect::<Vec<_>>();
                        (events, receiver)
                    },
                    |(events, receiver)| {
                        let builder = KinesisRequestBuilder::<KinesisStreamRecord> {
                            compression: Compression::None,
                            encoder: (
                                Transformer::default(),
                                Encoder::<()>::new(TextSerializerConfig::default().build().into()),
                            ),
                            _phantom: PhantomData,
                        };
                        runtime.block_on(async {
                            let mut output = requests(
                                futures::stream::iter(events).boxed(),
                                builder,
                                None,
                                Settings {
                                    format,
                                    max_events: 500,
                                    max_bytes: 943_718,
                                    timeout: Duration::from_secs(1),
                                },
                            );
                            let mut original_logs = 0;
                            let mut bytes = 0;
                            while let Some(request) = output.next().await {
                                original_logs += request.get_metadata().event_count();
                                bytes += request.record.record.data().as_ref().len();
                                std::hint::black_box(&request);
                            }
                            assert_eq!(original_logs, count);
                            if format == Format::Newline {
                                assert_eq!(bytes, (size + 1) * count);
                            } else {
                                assert!(bytes > size * count);
                            }
                            std::hint::black_box(receiver.await);
                        });
                    },
                    BatchSize::SmallInput,
                );
            });
        }
    }
    group.finish();
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
