use std::time::SystemTime;

use chrono::prelude::*;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use vector::enrichment_tables::{
    file::File,
    geoip::{Geoip, GeoipConfig},
    mmdb::{Mmdb, MmdbConfig},
    Condition, Table,
};
use vector_lib::enrichment::Case;
use vrl::value::{ObjectMap, Value};

criterion_group!(
    name = benches;
    config = Criterion::default().noise_threshold(0.02).sample_size(10);
    targets = benchmark_enrichment_tables_file, benchmark_enrichment_tables_geoip, benchmark_enrichment_tables_mmdb
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
        Value::Timestamp(
            Utc.ymd(2013, row as u32 % 10 + 1, 15)
                .and_hms_opt(0, 0, 0)
                .expect("invalid timestamp"),
        )
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
                        from: Utc
                            .ymd(2013, 6, 1)
                            .and_hms_opt(0, 0, 0)
                            .expect("invalid timestamp"),
                        to: Utc
                            .ymd(2013, 7, 1)
                            .and_hms_opt(0, 0, 0)
                            .expect("invalid timestamp"),
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
            .map(|idx| {
                (
                    format!("field-{}", idx).into(),
                    column(idx, size - result_offset),
                )
            })
            .collect::<ObjectMap>();

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

fn benchmark_enrichment_tables_geoip(c: &mut Criterion) {
    let mut group = c.benchmark_group("enrichment_tables_geoip");
    let build = |path: &str| {
        Geoip::new(GeoipConfig {
            path: path.to_string(),
            locale: "en".to_string(),
        })
        .unwrap()
    };

    group.bench_function("enrichment_tables/geoip_isp", |b| {
        let table = build("tests/data/GeoIP2-ISP-Test.mmdb");
        let ip = "208.192.1.2";
        let mut expected = ObjectMap::new();
        expected.insert("autonomous_system_number".into(), 701i64.into());
        expected.insert(
            "autonomous_system_organization".into(),
            "MCI Communications Services, Inc. d/b/a Verizon Business".into(),
        );
        expected.insert("isp".into(), "Verizon Business".into());
        expected.insert("organization".into(), "Verizon Business".into());

        b.iter_batched(
            || (&table, ip, &expected),
            |(table, ip, expected)| {
                assert_eq!(
                    Ok(expected),
                    table
                        .find_table_row(
                            Case::Insensitive,
                            &[Condition::Equals {
                                field: "ip",
                                value: ip.into(),
                            }],
                            None,
                            None,
                        )
                        .as_ref()
                )
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("enrichment_tables/geoip_city", |b| {
        let table = build("tests/data/GeoIP2-City-Test.mmdb");
        let ip = "67.43.156.9";
        let mut expected = ObjectMap::new();
        expected.insert("city_name".into(), Value::Null);
        expected.insert("country_code".into(), "BT".into());
        expected.insert("country_name".into(), "Bhutan".into());
        expected.insert("continent_code".into(), "AS".into());
        expected.insert("region_code".into(), Value::Null);
        expected.insert("region_name".into(), Value::Null);
        expected.insert("timezone".into(), "Asia/Thimphu".into());
        expected.insert("latitude".into(), Value::from(27.5));
        expected.insert("longitude".into(), Value::from(90.5));
        expected.insert("postal_code".into(), Value::Null);
        expected.insert("metro_code".into(), Value::Null);

        b.iter_batched(
            || (&table, ip, &expected),
            |(table, ip, expected)| {
                assert_eq!(
                    Ok(expected),
                    table
                        .find_table_row(
                            Case::Insensitive,
                            &[Condition::Equals {
                                field: "ip",
                                value: ip.into(),
                            }],
                            None,
                            None,
                        )
                        .as_ref()
                )
            },
            BatchSize::SmallInput,
        );
    });
}

fn benchmark_enrichment_tables_mmdb(c: &mut Criterion) {
    let mut group = c.benchmark_group("enrichment_tables_mmdb");
    let build = |path: &str| {
        Mmdb::new(MmdbConfig {
            path: path.to_string(),
        })
        .unwrap()
    };

    group.bench_function("enrichment_tables/mmdb_isp", |b| {
        let table = build("tests/data/GeoIP2-ISP-Test.mmdb");
        let ip = "208.192.1.2";
        let mut expected = ObjectMap::new();
        expected.insert("autonomous_system_number".into(), 701i64.into());
        expected.insert(
            "autonomous_system_organization".into(),
            "MCI Communications Services, Inc. d/b/a Verizon Business".into(),
        );
        expected.insert("isp".into(), "Verizon Business".into());
        expected.insert("organization".into(), "Verizon Business".into());

        b.iter_batched(
            || (&table, ip, &expected),
            |(table, ip, expected)| {
                assert_eq!(
                    Ok(expected),
                    table
                        .find_table_row(
                            Case::Insensitive,
                            &[Condition::Equals {
                                field: "ip",
                                value: ip.into(),
                            }],
                            None,
                            None,
                        )
                        .as_ref()
                )
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("enrichment_tables/mmdb_city", |b| {
        let table = build("tests/data/GeoIP2-City-Test.mmdb");
        let ip = "67.43.156.9";
        let mut expected = ObjectMap::new();
        expected.insert(
            "location".into(),
            ObjectMap::from([
                ("latitude".into(), Value::from(27.5)),
                ("longitude".into(), Value::from(90.5)),
            ])
            .into(),
        );

        b.iter_batched(
            || (&table, ip, &expected),
            |(table, ip, expected)| {
                assert_eq!(
                    Ok(expected),
                    table
                        .find_table_row(
                            Case::Insensitive,
                            &[Condition::Equals {
                                field: "ip",
                                value: ip.into(),
                            }],
                            Some(&[
                                "location.latitude".to_string(),
                                "location.longitude".to_string(),
                            ]),
                            None,
                        )
                        .as_ref()
                )
            },
            BatchSize::SmallInput,
        );
    });
}
