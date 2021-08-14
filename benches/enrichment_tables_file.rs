use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::collections::BTreeMap;
use vector::enrichment_tables::{file::File, EnrichmentTable};

criterion_group!(
    name = benches;
    config = Criterion::default().noise_threshold(0.02);
    targets = benchmark_enrichment_tables_file
);
criterion_main!(benches);

fn benchmark_enrichment_tables_file(c: &mut Criterion) {
    let mut group = c.benchmark_group("enrichment_tables_file");

    let setup = || {
        let mut file = File::new(
            (0..1000)
                .map(|idx| vec![format!("field{}", idx)])
                .collect::<Vec<_>>(),
            vec!["field".to_string()],
        );
        let index = file.add_index(vec!["field"]).unwrap();
        let mut condition = BTreeMap::new();
        condition.insert("field", "field999".to_string());

        let mut result = BTreeMap::new();
        result.insert("field".to_string(), "field999".to_string());

        (file, index, condition, result)
    };

    group.bench_function("enrichment_tables/file_noindex", |b| {
        b.iter_batched(
            || setup(),
            |(file, _index, condition, expected)| {
                assert_eq!(Some(expected), file.find_table_row(condition, None))
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("enrichment_tables/file_hashindex", |b| {
        b.iter_batched(
            || setup(),
            |(file, index, condition, expected)| {
                let mut result = BTreeMap::new();
                result.insert("field".to_string(), "field999".to_string());
                assert_eq!(Some(expected), file.find_table_row(condition, Some(index)))
            },
            BatchSize::SmallInput,
        );
    });
}
