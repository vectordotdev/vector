use std::{collections::BTreeMap, time::SystemTime};

use chrono::prelude::*;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use enrichment::Case;
use vector::enrichment_tables::{file::File, Condition, Table};
use vrl::Value;

criterion_group!(
    name = benches;
    config = Criterion::default().noise_threshold(0.02).sample_size(10);
    targets = benchmark_enrichment_tables_file
);
criterion_main!(benches);

/// Returns the text of the column at the given position.
fn column(col: usize, row: usize) -> Value {
    if col == 0 {
        // A column that is duplicated across 10 rows.
        Value::from(format!("data-0-{}", row / 10 * 10))
    } else if col == 1 {
        // And a final column with a date, each of the above duplicated row should have
        // a unique date.
        Value::Timestamp(Utc.ymd(2013, row as u32 % 10 + 1, 15).and_hms(0, 0, 0))
    } else {
        Value::from(format!("data-{}-{}", col, row))
    }
}

fn benchmark_enrichment_tables_file(c: &mut Criterion) {
    let mut group = c.benchmark_group("enrichment_tables_file");

    let setup = |size, date_range, case| {
        let data = (0..size)
            .map(|row| {
                // Add 8 columns.
                (0..10).map(|col| column(col, row)).collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let mut file = File::new(
            Default::default(),
            SystemTime::now(),
            data,
            // Headers.
            (0..10)
                .map(|header| format!("field-{}", header))
                .collect::<Vec<_>>(),
        );

        let (condition, index, result_offset) = if date_range {
            // Search on the first and last field.
            (
                vec![
                    Condition::Equals {
                        field: "field-0",
                        value: Value::from(format!("data-0-{}", (size - 1) / 10 * 10)),
                    },
                    Condition::BetweenDates {
                        field: "field-1",
                        from: Utc.ymd(2013, 6, 1).and_hms(0, 0, 0),
                        to: Utc.ymd(2013, 7, 1).and_hms(0, 0, 0),
                    },
                ],
                file.add_index(case, &["field-0"]).unwrap(),
                5,
            )
        } else {
            (
                vec![
                    Condition::Equals {
                        field: "field-2",
                        value: Value::from(format!("data-2-{}", size - 1)),
                    },
                    Condition::Equals {
                        field: "field-9",
                        value: Value::from(format!("data-9-{}", size - 1)),
                    },
                ],
                file.add_index(case, &["field-2", "field-9"]).unwrap(),
                1,
            )
        };

        let result = (0..10)
            .map(|idx| (format!("field-{}", idx), column(idx, size - result_offset)))
            .collect::<BTreeMap<_, _>>();

        (file, index, condition, result)
    };

    group.bench_function("enrichment_tables/file_date_10", |b| {
        let (file, index, condition, expected) = setup(10, true, Case::Sensitive);
        b.iter_batched(
            || (&file, &condition, expected.clone()),
            |(file, condition, expected)| {
                assert_eq!(
                    Ok(expected),
                    file.find_table_row(Case::Sensitive, condition, None, Some(index))
                )
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("enrichment_tables/file_hashindex_sensitive_10", |b| {
        let (file, index, condition, expected) = setup(10, false, Case::Sensitive);
        b.iter_batched(
            || (&file, index, &condition, expected.clone()),
            |(file, index, condition, expected)| {
                assert_eq!(
                    Ok(expected),
                    file.find_table_row(Case::Sensitive, condition, None, Some(index))
                )
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("enrichment_tables/file_hashindex_insensitive_10", |b| {
        let (file, index, condition, expected) = setup(10, false, Case::Insensitive);
        b.iter_batched(
            || (&file, index, &condition, expected.clone()),
            |(file, index, condition, expected)| {
                assert_eq!(
                    Ok(expected),
                    file.find_table_row(Case::Insensitive, condition, None, Some(index))
                )
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("enrichment_tables/file_date_1_000", |b| {
        let (file, index, condition, expected) = setup(1_000, true, Case::Sensitive);
        b.iter_batched(
            || (&file, &condition, expected.clone()),
            |(file, condition, expected)| {
                assert_eq!(
                    Ok(expected),
                    file.find_table_row(Case::Sensitive, condition, None, Some(index))
                )
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("enrichment_tables/file_hashindex_sensitive_1_000", |b| {
        let (file, index, condition, expected) = setup(1_000, false, Case::Sensitive);
        b.iter_batched(
            || (&file, index, &condition, expected.clone()),
            |(file, index, condition, expected)| {
                assert_eq!(
                    Ok(expected),
                    file.find_table_row(Case::Sensitive, condition, None, Some(index))
                )
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("enrichment_tables/file_hashindex_insensitive_1_000", |b| {
        let (file, index, condition, expected) = setup(1_000, false, Case::Insensitive);
        b.iter_batched(
            || (&file, index, &condition, expected.clone()),
            |(file, index, condition, expected)| {
                assert_eq!(
                    Ok(expected),
                    file.find_table_row(Case::Insensitive, condition, None, Some(index))
                )
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("enrichment_tables/file_date_1_000_000", |b| {
        let (file, index, condition, expected) = setup(1_000_000, true, Case::Sensitive);
        b.iter_batched(
            || (&file, &condition, expected.clone()),
            |(file, condition, expected)| {
                assert_eq!(
                    Ok(expected),
                    file.find_table_row(Case::Sensitive, condition, None, Some(index))
                )
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function(
        "enrichment_tables/file_hashindex_sensitive_1_000_000",
        |b| {
            let (file, index, condition, expected) = setup(1_000_000, false, Case::Sensitive);
            b.iter_batched(
                || (&file, index, &condition, expected.clone()),
                |(file, index, condition, expected)| {
                    assert_eq!(
                        Ok(expected),
                        file.find_table_row(Case::Sensitive, condition, None, Some(index))
                    )
                },
                BatchSize::SmallInput,
            );
        },
    );

    group.bench_function(
        "enrichment_tables/file_hashindex_insensitive_1_000_000",
        |b| {
            let (file, index, condition, expected) = setup(1_000_000, false, Case::Insensitive);
            b.iter_batched(
                || (&file, index, &condition, expected.clone()),
                |(file, index, condition, expected)| {
                    assert_eq!(
                        Ok(expected),
                        file.find_table_row(Case::Insensitive, condition, None, Some(index))
                    )
                },
                BatchSize::SmallInput,
            );
        },
    );
}
