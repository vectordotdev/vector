use super::*;
use crate::{btreemap, buckets, quantiles, samples};
use chrono::{offset::TimeZone, DateTime, Utc};
use std::str::FromStr;
use vrl::{Path, Value};

fn ts() -> DateTime<Utc> {
    Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11)
}

fn tags() -> MetricTags {
    vec![
        ("normal_tag".to_owned(), "value".to_owned()),
        ("true_tag".to_owned(), "true".to_owned()),
        ("empty_tag".to_owned(), "".to_owned()),
    ]
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vrl::Target;

    #[test]
    fn merge_counters() {
        let mut counter = Metric::new(
            "counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        );

        let delta = Metric::new(
            "counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 2.0 },
        )
        .with_namespace(Some("vector"))
        .with_tags(Some(tags()))
        .with_timestamp(Some(ts()));

        counter.data.add(&delta.data);
        assert_eq!(
            counter,
            Metric::new(
                "counter",
                MetricKind::Incremental,
                MetricValue::Counter { value: 3.0 },
            )
            .with_timestamp(Some(ts()))
        )
    }

    #[test]
    fn merge_gauges() {
        let mut gauge = Metric::new(
            "gauge",
            MetricKind::Incremental,
            MetricValue::Gauge { value: 1.0 },
        );

        let delta = Metric::new(
            "gauge",
            MetricKind::Incremental,
            MetricValue::Gauge { value: -2.0 },
        )
        .with_namespace(Some("vector"))
        .with_tags(Some(tags()))
        .with_timestamp(Some(ts()));

        gauge.data.add(&delta.data);
        assert_eq!(
            gauge,
            Metric::new(
                "gauge",
                MetricKind::Incremental,
                MetricValue::Gauge { value: -1.0 },
            )
            .with_timestamp(Some(ts()))
        )
    }

    #[test]
    fn merge_sets() {
        let mut set = Metric::new(
            "set",
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["old".into()].into_iter().collect(),
            },
        );

        let delta = Metric::new(
            "set",
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["new".into()].into_iter().collect(),
            },
        )
        .with_namespace(Some("vector"))
        .with_tags(Some(tags()))
        .with_timestamp(Some(ts()));

        set.data.add(&delta.data);
        assert_eq!(
            set,
            Metric::new(
                "set",
                MetricKind::Incremental,
                MetricValue::Set {
                    values: vec!["old".into(), "new".into()].into_iter().collect()
                },
            )
            .with_timestamp(Some(ts()))
        )
    }

    #[test]
    fn merge_histograms() {
        let mut dist = Metric::new(
            "hist",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: samples![1.0 => 10],
                statistic: StatisticKind::Histogram,
            },
        );

        let delta = Metric::new(
            "hist",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: samples![1.0 => 20],
                statistic: StatisticKind::Histogram,
            },
        )
        .with_namespace(Some("vector"))
        .with_tags(Some(tags()))
        .with_timestamp(Some(ts()));

        dist.data.add(&delta.data);
        assert_eq!(
            dist,
            Metric::new(
                "hist",
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: samples![1.0 => 10, 1.0 => 20],
                    statistic: StatisticKind::Histogram
                },
            )
            .with_timestamp(Some(ts()))
        )
    }

    #[test]
    fn display() {
        assert_eq!(
            format!(
                "{}",
                Metric::new(
                    "one",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1.23 },
                )
                .with_tags(Some(tags()))
            ),
            r#"one{empty_tag="",normal_tag="value",true_tag="true"} = 1.23"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric::new(
                    "two word",
                    MetricKind::Incremental,
                    MetricValue::Gauge { value: 2.0 }
                )
                .with_timestamp(Some(ts()))
            ),
            r#"2018-11-14T08:09:10.000000011Z "two word"{} + 2"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric::new(
                    "namespace",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1.23 },
                )
                .with_namespace(Some("vector"))
            ),
            r#"vector_namespace{} = 1.23"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric::new(
                    "namespace",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1.23 },
                )
                .with_namespace(Some("vector host"))
            ),
            r#""vector host"_namespace{} = 1.23"#
        );

        let mut values = BTreeSet::<String>::new();
        values.insert("v1".into());
        values.insert("v2_two".into());
        values.insert("thrəë".into());
        values.insert("four=4".into());
        assert_eq!(
            format!(
                "{}",
                Metric::new("three", MetricKind::Absolute, MetricValue::Set { values })
            ),
            r#"three{} = "four=4" "thrəë" v1 v2_two"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric::new(
                    "four",
                    MetricKind::Absolute,
                    MetricValue::Distribution {
                        samples: samples![1.0 => 3, 2.0 => 4],
                        statistic: StatisticKind::Histogram,
                    }
                )
            ),
            r#"four{} = histogram 3@1 4@2"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric::new(
                    "five",
                    MetricKind::Absolute,
                    MetricValue::AggregatedHistogram {
                        buckets: buckets![51.0 => 53, 52.0 => 54],
                        count: 107,
                        sum: 103.0,
                    }
                )
            ),
            r#"five{} = count=107 sum=103 53@51 54@52"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric::new(
                    "six",
                    MetricKind::Absolute,
                    MetricValue::AggregatedSummary {
                        quantiles: quantiles![1.0 => 63.0, 2.0 => 64.0],
                        count: 2,
                        sum: 127.0,
                    }
                )
            ),
            r#"six{} = count=2 sum=127 1@63 2@64"#
        );
    }

    #[test]
    fn object_metric_all_fields() {
        let metric = Metric::new(
            "zub",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.23 },
        )
        .with_namespace(Some("zoob"))
        .with_tags(Some({
            let mut map = MetricTags::new();
            map.insert("tig".to_string(), "tog".to_string());
            map
        }))
        .with_timestamp(Some(Utc.ymd(2020, 12, 10).and_hms(12, 0, 0)));

        assert_eq!(
            Ok(Some(
                btreemap! {
                    "name" => "zub",
                    "namespace" => "zoob",
                    "timestamp" => Utc.ymd(2020, 12, 10).and_hms(12, 0, 0),
                    "tags" => btreemap! { "tig" => "tog" },
                    "kind" => "absolute",
                    "type" => "counter",
                }
                .into()
            )),
            metric.get(&Path::from_str(".").unwrap())
        );
    }

    #[test]
    fn object_metric_fields() {
        let mut metric = Metric::new(
            "name",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.23 },
        )
        .with_tags(Some({
            let mut map = MetricTags::new();
            map.insert("tig".to_string(), "tog".to_string());
            map
        }));

        let cases = vec![
            (
                "name",                    // Path
                Some(Value::from("name")), // Current value
                Value::from("namefoo"),    // New value
                false,                     // Test deletion
            ),
            ("namespace", None, "namespacefoo".into(), true),
            (
                "timestamp",
                None,
                Utc.ymd(2020, 12, 8).and_hms(12, 0, 0).into(),
                true,
            ),
            (
                "kind",
                Some(Value::from("absolute")),
                "incremental".into(),
                false,
            ),
            ("tags.thing", None, "footag".into(), true),
        ];

        for (path, current, new, delete) in cases {
            let path = Path::from_str(path).unwrap();

            assert_eq!(Ok(current), metric.get(&path));
            assert_eq!(Ok(()), metric.insert(&path, new.clone()));
            assert_eq!(Ok(Some(new.clone())), metric.get(&path));

            if delete {
                assert_eq!(Ok(Some(new)), metric.remove(&path, true));
                assert_eq!(Ok(None), metric.get(&path));
            }
        }
    }

    #[test]
    fn object_metric_invalid_paths() {
        let mut metric = Metric::new(
            "name",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.23 },
        );

        let validpaths_get = vec![
            ".name",
            ".namespace",
            ".timestamp",
            ".kind",
            ".tags",
            ".type",
        ];

        let validpaths_set = vec![".name", ".namespace", ".timestamp", ".kind", ".tags"];

        assert_eq!(
            Err(format!(
                "invalid path .zork: expected one of {}",
                validpaths_get.join(", ")
            )),
            metric.get(&Path::from_str("zork").unwrap())
        );

        assert_eq!(
            Err(format!(
                "invalid path .zork: expected one of {}",
                validpaths_set.join(", ")
            )),
            metric.insert(&Path::from_str("zork").unwrap(), "thing".into())
        );

        assert_eq!(
            Err(format!(
                "invalid path .zork: expected one of {}",
                validpaths_set.join(", ")
            )),
            metric.remove(&Path::from_str("zork").unwrap(), true)
        );

        assert_eq!(
            Err(format!(
                "invalid path .tags.foo.flork: expected one of {}",
                validpaths_get.join(", ")
            )),
            metric.get(&Path::from_str("tags.foo.flork").unwrap())
        );
    }
}
