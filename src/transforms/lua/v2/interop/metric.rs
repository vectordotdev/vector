use super::util::{table_to_set, table_to_timestamp, timestamp_to_table};
use crate::event::{metric, Metric, MetricKind, MetricValue, StatisticKind};
use rlua::prelude::*;
use std::collections::BTreeMap;

impl<'a> ToLua<'a> for MetricKind {
    fn to_lua(self, ctx: LuaContext<'a>) -> LuaResult<LuaValue> {
        let kind = match self {
            MetricKind::Absolute => "absolute",
            MetricKind::Incremental => "incremental",
        };
        ctx.create_string(kind).map(LuaValue::String)
    }
}

impl<'a> FromLua<'a> for MetricKind {
    fn from_lua(value: LuaValue<'a>, _: LuaContext<'a>) -> LuaResult<Self> {
        match value {
            LuaValue::String(s) if s == "absolute" => Ok(MetricKind::Absolute),
            LuaValue::String(s) if s == "incremental" => Ok(MetricKind::Incremental),
            _ => Err(LuaError::FromLuaConversionError {
                from: value.type_name(),
                to: "MetricKind",
                message: Some(
                    "Metric kind should be either \"incremental\" or \"absolute\"".to_string(),
                ),
            }),
        }
    }
}

impl<'a> ToLua<'a> for StatisticKind {
    fn to_lua(self, ctx: LuaContext<'a>) -> LuaResult<LuaValue> {
        let kind = match self {
            StatisticKind::Summary => "summary",
            StatisticKind::Histogram => "histogram",
        };
        ctx.create_string(kind).map(LuaValue::String)
    }
}

impl<'a> FromLua<'a> for StatisticKind {
    fn from_lua(value: LuaValue<'a>, _: LuaContext<'a>) -> LuaResult<Self> {
        match value {
            LuaValue::String(s) if s == "summary" => Ok(StatisticKind::Summary),
            LuaValue::String(s) if s == "histogram" => Ok(StatisticKind::Histogram),
            _ => Err(LuaError::FromLuaConversionError {
                from: value.type_name(),
                to: "StatisticKind",
                message: Some(
                    "Statistic kind should be either \"summary\" or \"histogram\"".to_string(),
                ),
            }),
        }
    }
}

impl<'a> ToLua<'a> for Metric {
    fn to_lua(self, ctx: LuaContext<'a>) -> LuaResult<LuaValue> {
        let tbl = ctx.create_table()?;

        tbl.set("name", self.name())?;
        if let Some(ts) = self.data.timestamp {
            tbl.set("timestamp", timestamp_to_table(ctx, ts)?)?;
        }
        if let Some(tags) = self.series.tags {
            tbl.set("tags", tags)?;
        }
        tbl.set("kind", self.data.kind)?;

        match self.data.value {
            MetricValue::Counter { value } => {
                let counter = ctx.create_table()?;
                counter.set("value", value)?;
                tbl.set("counter", counter)?;
            }
            MetricValue::Gauge { value } => {
                let gauge = ctx.create_table()?;
                gauge.set("value", value)?;
                tbl.set("gauge", gauge)?;
            }
            MetricValue::Set { values } => {
                let set = ctx.create_table()?;
                set.set("values", ctx.create_sequence_from(values.into_iter())?)?;
                tbl.set("set", set)?;
            }
            MetricValue::Distribution { samples, statistic } => {
                let distribution = ctx.create_table()?;
                let sample_rates: Vec<_> = samples.iter().map(|s| s.rate).collect();
                let values: Vec<_> = samples.into_iter().map(|s| s.value).collect();
                distribution.set("values", values)?;
                distribution.set("sample_rates", sample_rates)?;
                distribution.set("statistic", statistic)?;
                tbl.set("distribution", distribution)?;
            }
            MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            } => {
                let aggregated_histogram = ctx.create_table()?;
                let counts: Vec<_> = buckets.iter().map(|b| b.count).collect();
                let buckets: Vec<_> = buckets.into_iter().map(|b| b.upper_limit).collect();
                aggregated_histogram.set("buckets", buckets)?;
                aggregated_histogram.set("counts", counts)?;
                aggregated_histogram.set("count", count)?;
                aggregated_histogram.set("sum", sum)?;
                tbl.set("aggregated_histogram", aggregated_histogram)?;
            }
            MetricValue::AggregatedSummary {
                quantiles,
                count,
                sum,
            } => {
                let aggregated_summary = ctx.create_table()?;
                let values: Vec<_> = quantiles.iter().map(|q| q.value).collect();
                let quantiles: Vec<_> = quantiles.into_iter().map(|q| q.upper_limit).collect();
                aggregated_summary.set("quantiles", quantiles)?;
                aggregated_summary.set("values", values)?;
                aggregated_summary.set("count", count)?;
                aggregated_summary.set("sum", sum)?;
                tbl.set("aggregated_summary", aggregated_summary)?;
            }
        }

        Ok(LuaValue::Table(tbl))
    }
}

impl<'a> FromLua<'a> for Metric {
    fn from_lua(value: LuaValue<'a>, _: LuaContext<'a>) -> LuaResult<Self> {
        let table = match &value {
            LuaValue::Table(table) => table,
            other => {
                return Err(LuaError::FromLuaConversionError {
                    from: other.type_name(),
                    to: "Metric",
                    message: Some("Metric should be a Lua table".to_string()),
                })
            }
        };

        let name: String = table.get("name")?;
        let timestamp = table
            .get::<_, Option<LuaTable>>("timestamp")?
            .map(table_to_timestamp)
            .transpose()?;
        let tags: Option<BTreeMap<String, String>> = table.get("tags")?;
        let kind = table
            .get::<_, Option<MetricKind>>("kind")?
            .unwrap_or(MetricKind::Absolute);

        let value = if let Some(counter) = table.get::<_, Option<LuaTable>>("counter")? {
            MetricValue::Counter {
                value: counter.get("value")?,
            }
        } else if let Some(gauge) = table.get::<_, Option<LuaTable>>("gauge")? {
            MetricValue::Gauge {
                value: gauge.get("value")?,
            }
        } else if let Some(set) = table.get::<_, Option<LuaTable>>("set")? {
            MetricValue::Set {
                values: set.get::<_, LuaTable>("values").and_then(table_to_set)?,
            }
        } else if let Some(distribution) = table.get::<_, Option<LuaTable>>("distribution")? {
            let values: Vec<f64> = distribution.get("values")?;
            let rates: Vec<u32> = distribution.get("sample_rates")?;
            MetricValue::Distribution {
                samples: metric::zip_samples(values, rates),
                statistic: distribution.get("statistic")?,
            }
        } else if let Some(aggregated_histogram) =
            table.get::<_, Option<LuaTable>>("aggregated_histogram")?
        {
            let counts: Vec<u32> = aggregated_histogram.get("counts")?;
            let buckets: Vec<f64> = aggregated_histogram.get("buckets")?;
            let count = counts.iter().sum();
            MetricValue::AggregatedHistogram {
                buckets: metric::zip_buckets(buckets, counts),
                count,
                sum: aggregated_histogram.get("sum")?,
            }
        } else if let Some(aggregated_summary) =
            table.get::<_, Option<LuaTable>>("aggregated_summary")?
        {
            let quantiles: Vec<f64> = aggregated_summary.get("quantiles")?;
            let values: Vec<f64> = aggregated_summary.get("values")?;
            MetricValue::AggregatedSummary {
                quantiles: metric::zip_quantiles(quantiles, values),
                count: aggregated_summary.get("count")?,
                sum: aggregated_summary.get("sum")?,
            }
        } else {
            return Err(LuaError::FromLuaConversionError {
                from: value.type_name(),
                to: "Metric",
                message: Some("Cannot find metric value, expected presence one of \"counter\", \"gauge\", \"set\", \"distribution\", \"aggregated_histogram\", \"aggregated_summary\"".to_string()),
            });
        };

        Ok(Metric::new(name, None, timestamp, tags, kind, value))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::{offset::TimeZone, Utc};

    fn assert_metric(metric: Metric, assertions: Vec<&'static str>) {
        Lua::new().context(|ctx| {
            ctx.globals().set("metric", metric).unwrap();
            for assertion in assertions {
                assert!(
                    ctx.load(assertion).eval::<bool>().expect(assertion),
                    assertion
                );
            }
        });
    }

    #[test]
    fn to_lua_counter_full() {
        let metric = Metric::new(
            "example counter".into(),
            None,
            Some(Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11)),
            Some(
                vec![("example tag".to_string(), "example value".to_string())]
                    .into_iter()
                    .collect(),
            ),
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        );
        let assertions = vec![
            "type(metric) == 'table'",
            "metric.name == 'example counter'",
            "type(metric.timestamp) == 'table'",
            "metric.timestamp.year == 2018",
            "metric.timestamp.month == 11",
            "metric.timestamp.day == 14",
            "metric.timestamp.hour == 8",
            "metric.timestamp.min == 9",
            "metric.timestamp.sec == 10",
            "type(metric.tags) == 'table'",
            "metric.tags['example tag'] == 'example value'",
            "metric.kind == 'incremental'",
            "type(metric.counter) == 'table'",
            "metric.counter.value == 1",
        ];
        assert_metric(metric, assertions);
    }

    #[test]
    fn to_lua_counter_minimal() {
        let metric = Metric::new(
            "example counter".into(),
            None,
            None,
            None,
            MetricKind::Absolute,
            MetricValue::Counter { value: 0.57721566 },
        );
        let assertions = vec![
            "metric.timestamp == nil",
            "metric.tags == nil",
            "metric.kind == 'absolute'",
            "metric.counter.value == 0.57721566",
        ];
        assert_metric(metric, assertions);
    }

    #[test]
    fn to_lua_gauge() {
        let metric = Metric::new(
            "example gauge".into(),
            None,
            None,
            None,
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.6180339 },
        );
        let assertions = vec!["metric.gauge.value == 1.6180339", "metric.counter == nil"];
        assert_metric(metric, assertions);
    }

    #[test]
    fn to_lua_set() {
        let metric = Metric::new(
            "example set".into(),
            None,
            None,
            None,
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["value".into(), "another value".into()]
                    .into_iter()
                    .collect(),
            },
        );
        let assertions = vec![
            "type(metric.set) == 'table'",
            "type(metric.set.values) == 'table'",
            "#metric.set.values == 2",
            "metric.set.values[1] == 'another value'",
            "metric.set.values[2] == 'value'",
        ];
        assert_metric(metric, assertions);
    }

    #[test]
    fn to_lua_distribution() {
        let metric = Metric::new(
            "example distribution".into(),
            None,
            None,
            None,
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: crate::samples![1.0 => 10, 1.0 => 20],
                statistic: StatisticKind::Histogram,
            },
        );
        let assertions = vec![
            "type(metric.distribution) == 'table'",
            "#metric.distribution.values == 2",
            "metric.distribution.values[1] == 1",
            "metric.distribution.values[2] == 1",
            "#metric.distribution.sample_rates == 2",
            "metric.distribution.sample_rates[1] == 10",
            "metric.distribution.sample_rates[2] == 20",
        ];
        assert_metric(metric, assertions)
    }

    #[test]
    fn to_lua_aggregated_histogram() {
        let metric = Metric::new(
            "example histogram".into(),
            None,
            None,
            None,
            MetricKind::Incremental,
            MetricValue::AggregatedHistogram {
                buckets: crate::buckets![1.0 => 20, 2.0 => 10, 4.0 => 45, 8.0 => 12],
                count: 87,
                sum: 975.2,
            },
        );
        let assertions = vec![
            "type(metric.aggregated_histogram) == 'table'",
            "#metric.aggregated_histogram.buckets == 4",
            "metric.aggregated_histogram.buckets[1] == 1",
            "metric.aggregated_histogram.buckets[4] == 8",
            "#metric.aggregated_histogram.counts == 4",
            "metric.aggregated_histogram.counts[1] == 20",
            "metric.aggregated_histogram.counts[4] == 12",
            "metric.aggregated_histogram.count == 87",
            "metric.aggregated_histogram.sum == 975.2",
        ];
        assert_metric(metric, assertions)
    }

    #[test]
    fn to_lua_aggregated_summary() {
        let metric = Metric::new(
            "example summary".into(),
            None,
            None,
            None,
            MetricKind::Incremental,
            MetricValue::AggregatedSummary {
                quantiles: crate::quantiles![
                    0.1 => 2.0, 0.25 => 3.0, 0.5 => 5.0, 0.75 => 8.0, 0.9 => 7.0, 0.99 => 9.0, 1.0 => 10.0
                ],
                count: 197,
                sum: 975.2,
            },
        );
        let assertions = vec![
            "type(metric.aggregated_summary) == 'table'",
            "#metric.aggregated_summary.quantiles == 7",
            "metric.aggregated_summary.quantiles[2] == 0.25",
            "#metric.aggregated_summary.values == 7",
            "metric.aggregated_summary.values[3] == 5",
            "metric.aggregated_summary.count == 197",
            "metric.aggregated_summary.sum == 975.2",
        ];
        assert_metric(metric, assertions)
    }

    #[test]
    fn from_lua_counter_minimal() {
        let value = r#"{
            name = "example counter",
            counter = {
                value = 0.57721566
            }
        }"#;
        let expected = Metric::new(
            "example counter".into(),
            None,
            None,
            None,
            MetricKind::Absolute,
            MetricValue::Counter { value: 0.57721566 },
        );
        Lua::new().context(|ctx| {
            assert_eq!(ctx.load(value).eval::<Metric>().unwrap(), expected);
        });
    }

    #[test]
    fn from_lua_counter_full() {
        let value = r#"{
            name = "example counter",
            timestamp = {
                year = 2018,
                month = 11,
                day = 14,
                hour = 8,
                min = 9,
                sec = 10
            },
            tags = {
                ["example tag"] = "example value"
            },
            kind = "incremental",
            counter = {
                value = 1
            }
        }"#;
        let expected = Metric::new(
            "example counter".into(),
            None,
            Some(Utc.ymd(2018, 11, 14).and_hms(8, 9, 10)),
            Some(
                vec![("example tag".to_string(), "example value".to_string())]
                    .into_iter()
                    .collect(),
            ),
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        );
        Lua::new().context(|ctx| {
            assert_eq!(ctx.load(value).eval::<Metric>().unwrap(), expected);
        });
    }

    #[test]
    fn from_lua_gauge() {
        let value = r#"{
            name = "example gauge",
            gauge = {
                value = 1.6180339
            }
        }"#;
        let expected = Metric::new(
            "example gauge".into(),
            None,
            None,
            None,
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.6180339 },
        );
        Lua::new().context(|ctx| {
            assert_eq!(ctx.load(value).eval::<Metric>().unwrap(), expected);
        });
    }

    #[test]
    fn from_lua_set() {
        let value = r#"{
            name = "example set",
            set = {
                values = { "value", "another value" }
            }
        }"#;
        let expected = Metric::new(
            "example set".into(),
            None,
            None,
            None,
            MetricKind::Absolute,
            MetricValue::Set {
                values: vec!["value".into(), "another value".into()]
                    .into_iter()
                    .collect(),
            },
        );
        Lua::new().context(|ctx| {
            assert_eq!(ctx.load(value).eval::<Metric>().unwrap(), expected);
        });
    }

    #[test]
    fn from_lua_distribution() {
        let value = r#"{
            name = "example distribution",
            distribution = {
                values = { 1.0, 1.0 },
                sample_rates = { 10, 20 },
                statistic = "histogram"
            }
        }"#;
        let expected = Metric::new(
            "example distribution".into(),
            None,
            None,
            None,
            MetricKind::Absolute,
            MetricValue::Distribution {
                samples: crate::samples![1.0 => 10, 1.0 => 20],
                statistic: StatisticKind::Histogram,
            },
        );
        Lua::new().context(|ctx| {
            assert_eq!(ctx.load(value).eval::<Metric>().unwrap(), expected);
        });
    }

    #[test]
    fn from_lua_aggregated_histogram() {
        let value = r#"{
            name = "example histogram",
            aggregated_histogram = {
                buckets = { 1, 2, 4, 8 },
                counts = { 20, 10, 45, 12 },
                sum = 975.2
            }
        }"#;
        let expected = Metric::new(
            "example histogram".into(),
            None,
            None,
            None,
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                buckets: crate::buckets![1.0 => 20, 2.0 => 10, 4.0 => 45, 8.0 => 12],
                count: 87,
                sum: 975.2,
            },
        );
        Lua::new().context(|ctx| {
            assert_eq!(ctx.load(value).eval::<Metric>().unwrap(), expected);
        });
    }

    #[test]
    fn from_lua_aggregated_summary() {
        let value = r#"{
            name = "example summary",
            aggregated_summary = {
                quantiles = { 0.1, 0.25, 0.5, 0.75, 0.9, 0.99, 1.0 },
                values = { 2.0, 3.0, 5.0, 8.0, 7.0, 9.0, 10.0 },
                count = 197,
                sum = 975.2
            }
        }"#;
        let expected = Metric::new(
            "example summary".into(),
            None,
            None,
            None,
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles: crate::quantiles![
                    0.1 => 2.0, 0.25 => 3.0, 0.5 => 5.0, 0.75 => 8.0, 0.9 => 7.0, 0.99 => 9.0, 1.0 => 10.0
                ],
                count: 197,
                sum: 975.2,
            },
        );
        Lua::new().context(|ctx| {
            assert_eq!(ctx.load(value).eval::<Metric>().unwrap(), expected);
        });
    }
}
