use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use vector::{
    config::{DataType, Output},
    event::{EventArray, EventContainer, LogEvent},
    transforms::{
        remap::{Remap, RemapConfig},
        SyncTransform, TransformOutputsBuf,
    },
};
use vector_common::{btreemap, TimeZone};
use vrl::{prelude::*, VrlRuntime};

criterion_group!(
    name = benches;
    // encapsulates CI noise we saw in
    // https://github.com/vectordotdev/vector/issues/5394
    config = Criterion::default().noise_threshold(0.02);
    targets = bench
);
criterion_main!(benches);

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("remap_vrl_batched");

    group.bench_function("datadog_agent_remap_blackhole", |b| {
        let (mut remap, _) = Remap::new_ast_batch(
            RemapConfig {
                source: Some(
                    indoc! {r#"
                        .hostname = "vector"

                        if .status == "warning" {
                            .thing = upcase(.hostname)
                        } else if .status == "notice" {
                            .thung = downcase(.hostname)
                        } else {
                            .nong = upcase(.hostname)
                        }

                        .matches = { "name": .message, "num": "2" }
                        .origin, .err = .hostname + "/" + .matches.name + "/" + .matches.num
                    "#}
                    .into(),
                ),
                file: None,
                timezone: TimeZone::default(),
                drop_on_error: true,
                drop_on_abort: true,
                runtime: VrlRuntime::AstBatch,
                ..Default::default()
            },
            &Default::default(),
        )
        .unwrap();

        let batch = EventArray::Logs(vec![
            LogEvent::from(btreemap! {
                "status" => "warning"
            });
            1000
        ]);

        b.iter_batched(
            || {
                (
                    batch.clone(),
                    TransformOutputsBuf::new_with_capacity(
                        (0..batch.len())
                            .map(|_| Output::default(DataType::Log))
                            .collect(),
                        batch.len(),
                    ),
                )
            },
            |(batch, mut output)| {
                remap.transform_all(batch, &mut output);
                output
            },
            BatchSize::LargeInput,
        );
    });
}
