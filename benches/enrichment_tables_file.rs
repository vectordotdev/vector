use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::collections::BTreeMap;
use vector::enrichment_tables::{file::File, file::IndexingStrategy, EnrichmentTable};

criterion_group!(
    name = benches;
    config = Criterion::default().noise_threshold(0.02);
    targets = benchmark_enrichment_tables_file
);
criterion_main!(benches);

fn benchmark_enrichment_tables_file(c: &mut Criterion) {
    let mut group = c.benchmark_group("enrichment_tables_file");

    let setup = |strategy| {
        (
            {
                let mut file = File::new(
                    strategy,
                    (0..1000)
                        .map(|idx| vec![format!("field{}", idx)])
                        .collect::<Vec<_>>(),
                    vec!["field".to_string()],
                );
                file.add_index(vec!["field"]);
                file
            },
            {
                let mut condition = BTreeMap::new();
                condition.insert("field", "field999".to_string());
                condition
            },
            {
                let mut result = BTreeMap::new();
                result.insert("field".to_string(), "field999".to_string());
                result
            },
        )
    };

    group.bench_function("enrichment_tables/file_noindex", |b| {
        b.iter_batched(
            || setup(IndexingStrategy::None),
            |(file, condition, expected)| {
                assert_eq!(Some(expected), file.find_table_row(condition))
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("enrichment_tables/file_hashindex", |b| {
        b.iter_batched(
            || setup(IndexingStrategy::Hash),
            |(file, condition, expected)| {
                let mut result = BTreeMap::new();
                result.insert("field".to_string(), "field999".to_string());
                assert_eq!(Some(expected), file.find_table_row(condition))
            },
            BatchSize::SmallInput,
        );
    });
}
