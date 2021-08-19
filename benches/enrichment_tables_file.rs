use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::collections::BTreeMap;
use vector::enrichment_tables::{file::File, Condition, Table};

criterion_group!(
    name = benches;
    config = Criterion::default().noise_threshold(0.02).sample_size(10);
    targets = benchmark_enrichment_tables_file
);
criterion_main!(benches);

fn benchmark_enrichment_tables_file(c: &mut Criterion) {
    let mut group = c.benchmark_group("enrichment_tables_file");

    let setup = |size| {
        let mut file = File::new(
            // Data
            (0..size)
                .map(|row| {
                    // Add 10 columns
                    (0..10)
                        .map(|col| format!("data-{}-{}", col, row))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
            // Headers
            (0..10)
                .map(|header| format!("field-{}", header))
                .collect::<Vec<_>>(),
        );

        // Search on the first and last field.
        let index = file.add_index(&["field-0", "field-9"]).unwrap();

        let condition = vec![
            Condition::Equals {
                field: "field-0",
                value: format!("data-0-{}", size - 1),
            },
            Condition::Equals {
                field: "field-9",
                value: format!("data-9-{}", size - 1),
            },
        ];

        let result = (0..10)
            .map(|idx| {
                (
                    format!("field-{}", idx),
                    format!("data-{}-{}", idx, size - 1),
                )
            })
            .collect::<BTreeMap<_, _>>();

        (file, index, condition, result)
    };

    group.bench_function("enrichment_tables/file_noindex_10", |b| {
        let (file, _index, condition, expected) = setup(10);
        b.iter_batched(
            || (&file, &condition, expected.clone()),
            |(file, condition, expected)| {
                assert_eq!(Ok(expected), file.find_table_row(condition, None))
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("enrichment_tables/file_hashindex_10", |b| {
        let (file, index, condition, expected) = setup(10);
        b.iter_batched(
            || (&file, index, &condition, expected.clone()),
            |(file, index, condition, expected)| {
                assert_eq!(Ok(expected), file.find_table_row(condition, Some(index)))
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("enrichment_tables/file_noindex_1_000", |b| {
        let (file, _index, condition, expected) = setup(1_000);
        b.iter_batched(
            || (&file, &condition, expected.clone()),
            |(file, condition, expected)| {
                assert_eq!(Ok(expected), file.find_table_row(condition, None))
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("enrichment_tables/file_hashindex_1_000", |b| {
        let (file, index, condition, expected) = setup(1_000);
        b.iter_batched(
            || (&file, index, &condition, expected.clone()),
            |(file, index, condition, expected)| {
                assert_eq!(Ok(expected), file.find_table_row(condition, Some(index)))
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("enrichment_tables/file_noindex_1_000_000", |b| {
        let (file, _index, condition, expected) = setup(1_000_000);
        b.iter_batched(
            || (&file, &condition, expected.clone()),
            |(file, condition, expected)| {
                assert_eq!(Ok(expected), file.find_table_row(condition, None))
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("enrichment_tables/file_hashindex_1_000_000", |b| {
        let (file, index, condition, expected) = setup(1_000_000);
        b.iter_batched(
            || (&file, index, &condition, expected.clone()),
            |(file, index, condition, expected)| {
                assert_eq!(Ok(expected), file.find_table_row(condition, Some(index)))
            },
            BatchSize::SmallInput,
        );
    });
}
