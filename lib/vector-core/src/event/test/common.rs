use std::{collections::BTreeSet, iter};

use chrono::{DateTime, Utc};
use quickcheck::{empty_shrinker, Arbitrary, Gen};
use vrl::value::{ObjectMap, Value};

use super::super::{
    metric::{
        Bucket, MetricData, MetricName, MetricSeries, MetricSketch, MetricTags, MetricTime,
        Quantile, Sample,
    },
    Event, EventMetadata, LogEvent, Metric, MetricKind, MetricValue, StatisticKind, TraceEvent,
};
use crate::metrics::AgentDDSketch;

const MAX_F64_SIZE: f64 = 1_000_000.0;
const MAX_MAP_SIZE: usize = 4;
const MAX_STR_SIZE: usize = 16;
const ALPHABET: [&str; 27] = [
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t", "u", "v", "w", "x", "y", "z", "_",
];

#[derive(Debug, Clone)]
pub struct Name {
    inner: String,
}

impl Arbitrary for Name {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut name = String::with_capacity(MAX_STR_SIZE);
        for _ in 0..(g.size() % MAX_STR_SIZE) {
            let idx: usize = usize::arbitrary(g) % ALPHABET.len();
            name.push_str(ALPHABET[idx]);
        }

        Name { inner: name }
    }
}

impl From<Name> for String {
    fn from(name: Name) -> String {
        name.inner
    }
}

fn datetime(g: &mut Gen) -> DateTime<Utc> {
    // chrono documents that there is an out-of-range for both second and
    // nanosecond values but doesn't actually document what the valid ranges
    // are. We just sort of arbitrarily restrict things.
    let secs = i64::arbitrary(g) % 32_000;
    let nanosecs = u32::arbitrary(g) % 32_000;
    DateTime::from_timestamp(secs, nanosecs).expect("invalid timestamp")
}

impl Arbitrary for Event {
    fn arbitrary(g: &mut Gen) -> Self {
        let choice: u8 = u8::arbitrary(g);
        // Quickcheck can't derive Arbitrary for enums, see
        // https://github.com/BurntSushi/quickcheck/issues/98
        if choice % 2 == 0 {
            Event::Log(LogEvent::arbitrary(g))
        } else {
            Event::Metric(Metric::arbitrary(g))
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self {
            Event::Log(log_event) => Box::new(log_event.shrink().map(Event::Log)),
            Event::Metric(metric) => Box::new(metric.shrink().map(Event::Metric)),
            Event::Trace(trace) => Box::new(trace.shrink().map(Event::Trace)),
        }
    }
}

impl Arbitrary for LogEvent {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut gen = Gen::new(MAX_MAP_SIZE);
        let map: ObjectMap = ObjectMap::arbitrary(&mut gen);
        let metadata: EventMetadata = EventMetadata::arbitrary(g);
        LogEvent::from_map(map, metadata)
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let (value, metadata) = self.clone().into_parts();

        Box::new(
            value
                .shrink()
                .map(move |x| LogEvent::from_parts(x, metadata.clone())),
        )
    }
}

impl Arbitrary for TraceEvent {
    fn arbitrary(g: &mut Gen) -> Self {
        Self::from(LogEvent::arbitrary(g))
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let (fields, metadata) = self.clone().into_parts();

        Box::new(
            fields
                .shrink()
                .map(move |x| TraceEvent::from_parts(x, metadata.clone())),
        )
    }
}

impl Arbitrary for Metric {
    fn arbitrary(g: &mut Gen) -> Self {
        let name = String::from(Name::arbitrary(g));
        let kind = MetricKind::arbitrary(g);
        let value = MetricValue::arbitrary(g);
        let metadata = EventMetadata::arbitrary(g);
        let mut metric = Metric::new_with_metadata(name, kind, value, metadata);
        metric.data = MetricData::arbitrary(g);
        metric.series = MetricSeries::arbitrary(g);

        metric
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let metric = self.clone();
        let name = String::from(metric.name());

        Box::new(
            name.shrink()
                .map(move |name| metric.clone().with_name(name))
                .flat_map(|metric| {
                    let data = metric.data.clone();
                    data.shrink().map(move |data| {
                        let mut new_metric = metric.clone();
                        new_metric.data = data;
                        new_metric
                    })
                })
                .flat_map(|metric| {
                    let series = metric.series.clone();
                    series.shrink().map(move |series| {
                        let mut new_metric = metric.clone();
                        new_metric.series = series;
                        new_metric
                    })
                }),
        )
    }
}

impl Arbitrary for MetricKind {
    fn arbitrary(g: &mut Gen) -> Self {
        let choice: u8 = u8::arbitrary(g);
        // Quickcheck can't derive Arbitrary for enums, see
        // https://github.com/BurntSushi/quickcheck/issues/98
        if choice % 2 == 0 {
            MetricKind::Incremental
        } else {
            MetricKind::Absolute
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        empty_shrinker()
    }
}

impl Arbitrary for MetricValue {
    fn arbitrary(g: &mut Gen) -> Self {
        // Quickcheck can't derive Arbitrary for enums, see
        // https://github.com/BurntSushi/quickcheck/issues/98.  The magical
        // constant here are the number of fields in `MetricValue`. Because the
        // field total is not a power of two we introduce a bias into choice
        // here toward `MetricValue::Counter` and `MetricValue::Gauge`.
        match u8::arbitrary(g) % 7 {
            0 => MetricValue::Counter {
                value: f64::arbitrary(g) % MAX_F64_SIZE,
            },
            1 => MetricValue::Gauge {
                value: f64::arbitrary(g) % MAX_F64_SIZE,
            },
            2 => MetricValue::Set {
                values: BTreeSet::arbitrary(g),
            },
            3 => MetricValue::Distribution {
                samples: Vec::arbitrary(g),
                statistic: StatisticKind::arbitrary(g),
            },
            4 => MetricValue::AggregatedHistogram {
                buckets: Vec::arbitrary(g),
                count: u64::arbitrary(g),
                sum: f64::arbitrary(g) % MAX_F64_SIZE,
            },
            5 => MetricValue::AggregatedSummary {
                quantiles: Vec::arbitrary(g),
                count: u64::arbitrary(g),
                sum: f64::arbitrary(g) % MAX_F64_SIZE,
            },
            6 => {
                // We're working around quickcheck's limitations here, and
                // should really migrate the tests in question to use proptest
                let num_samples = u8::arbitrary(g);
                let samples = std::iter::repeat_with(|| loop {
                    let f = f64::arbitrary(g);
                    if f.is_normal() {
                        return f;
                    }
                })
                .take(num_samples as usize)
                .collect::<Vec<_>>();

                let mut sketch = AgentDDSketch::with_agent_defaults();
                sketch.insert_many(&samples);

                MetricValue::Sketch {
                    sketch: MetricSketch::AgentDDSketch(sketch),
                }
            }

            _ => unreachable!(),
        }
    }

    #[allow(clippy::too_many_lines)] // no real way to make this shorter
    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self {
            MetricValue::Counter { value } => {
                Box::new(value.shrink().map(|value| MetricValue::Counter { value }))
            }
            MetricValue::Gauge { value } => {
                Box::new(value.shrink().map(|value| MetricValue::Gauge { value }))
            }
            MetricValue::Set { values } => {
                Box::new(values.shrink().map(|values| MetricValue::Set { values }))
            }
            MetricValue::Distribution { samples, statistic } => {
                let statistic = *statistic;
                Box::new(
                    samples
                        .shrink()
                        .map(move |samples| MetricValue::Distribution { samples, statistic })
                        .flat_map(|metric_value| match metric_value {
                            MetricValue::Distribution { samples, statistic } => statistic
                                .shrink()
                                .map(move |statistic| MetricValue::Distribution {
                                    samples: samples.clone(),
                                    statistic,
                                }),
                            _ => unreachable!(),
                        }),
                )
            }
            MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            } => {
                let buckets = buckets.clone();
                let count = *count;
                let sum = *sum;

                Box::new(
                    buckets
                        .shrink()
                        .map(move |buckets| MetricValue::AggregatedHistogram {
                            buckets,
                            count,
                            sum,
                        })
                        .flat_map(move |hist| match hist {
                            MetricValue::AggregatedHistogram {
                                buckets,
                                count,
                                sum,
                            } => {
                                count
                                    .shrink()
                                    .map(move |count| MetricValue::AggregatedHistogram {
                                        buckets: buckets.clone(),
                                        count,
                                        sum,
                                    })
                            }
                            _ => unreachable!(),
                        })
                        .flat_map(move |hist| match hist {
                            MetricValue::AggregatedHistogram {
                                buckets,
                                count,
                                sum,
                            } => sum
                                .shrink()
                                .map(move |sum| MetricValue::AggregatedHistogram {
                                    buckets: buckets.clone(),
                                    count,
                                    sum,
                                }),
                            _ => unreachable!(),
                        }),
                )
            }
            MetricValue::AggregatedSummary {
                quantiles,
                count,
                sum,
            } => {
                let quantiles = quantiles.clone();
                let count = *count;
                let sum = *sum;

                Box::new(
                    quantiles
                        .shrink()
                        .map(move |quantiles| MetricValue::AggregatedSummary {
                            quantiles,
                            count,
                            sum,
                        })
                        .flat_map(move |hist| match hist {
                            MetricValue::AggregatedSummary {
                                quantiles,
                                count,
                                sum,
                            } => count
                                .shrink()
                                .map(move |count| MetricValue::AggregatedSummary {
                                    quantiles: quantiles.clone(),
                                    count,
                                    sum,
                                }),
                            _ => unreachable!(),
                        })
                        .flat_map(move |hist| match hist {
                            MetricValue::AggregatedSummary {
                                quantiles,
                                count,
                                sum,
                            } => sum.shrink().map(move |sum| MetricValue::AggregatedSummary {
                                quantiles: quantiles.clone(),
                                count,
                                sum,
                            }),
                            _ => unreachable!(),
                        }),
                )
            }
            // Property testing a sketch doesn't actually make any sense, I don't think.
            //
            // We can't extract the values used to build it, which is by design, so all we could do
            // is mess with the internal buckets, which isn't even exposed (and absolutely shouldn't
            // be) and doing that is guaranteed to mess with the sketch in non-obvious ways that
            // would not occur if we were actually seeding it with real samples.
            MetricValue::Sketch { sketch } => Box::new(iter::once(MetricValue::Sketch {
                sketch: sketch.clone(),
            })),
        }
    }
}

impl Arbitrary for Sample {
    fn arbitrary(g: &mut Gen) -> Self {
        Sample {
            value: f64::arbitrary(g) % MAX_F64_SIZE,
            rate: u32::arbitrary(g),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let base = *self;

        Box::new(
            base.value
                .shrink()
                .map(move |value| {
                    let mut sample = base;
                    sample.value = value;
                    sample
                })
                .flat_map(|sample| {
                    sample.rate.shrink().map(move |rate| {
                        let mut ns = sample;
                        ns.rate = rate;
                        ns
                    })
                }),
        )
    }
}

impl Arbitrary for Quantile {
    fn arbitrary(g: &mut Gen) -> Self {
        Quantile {
            quantile: f64::arbitrary(g) % MAX_F64_SIZE,
            value: f64::arbitrary(g) % MAX_F64_SIZE,
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let base = *self;

        Box::new(
            base.quantile
                .shrink()
                .map(move |upper_limit| {
                    let mut quantile = base;
                    quantile.quantile = upper_limit;
                    quantile
                })
                .flat_map(|quantile| {
                    quantile.value.shrink().map(move |value| {
                        let mut nq = quantile;
                        nq.value = value;
                        nq
                    })
                }),
        )
    }
}

impl Arbitrary for Bucket {
    fn arbitrary(g: &mut Gen) -> Self {
        Bucket {
            upper_limit: f64::arbitrary(g) % MAX_F64_SIZE,
            count: u64::arbitrary(g),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let base = *self;

        Box::new(
            base.upper_limit
                .shrink()
                .map(move |upper_limit| {
                    let mut nb = base;
                    nb.upper_limit = upper_limit;
                    nb
                })
                .flat_map(|bucket| {
                    bucket.count.shrink().map(move |count| {
                        let mut nb = bucket;
                        nb.count = count;
                        nb
                    })
                }),
        )
    }
}

impl Arbitrary for StatisticKind {
    fn arbitrary(g: &mut Gen) -> Self {
        let choice: u8 = u8::arbitrary(g);
        // Quickcheck can't derive Arbitrary for enums, see
        // https://github.com/BurntSushi/quickcheck/issues/98
        if choice % 2 == 0 {
            StatisticKind::Histogram
        } else {
            StatisticKind::Summary
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        empty_shrinker()
    }
}

impl Arbitrary for MetricSeries {
    fn arbitrary(g: &mut Gen) -> Self {
        let tags = if bool::arbitrary(g) {
            let mut map = MetricTags::default();
            for _ in 0..(usize::arbitrary(g) % MAX_MAP_SIZE) {
                let key = String::from(Name::arbitrary(g));
                let value = String::from(Name::arbitrary(g));
                map.replace(key, value);
            }
            if map.is_empty() {
                None
            } else {
                Some(map)
            }
        } else {
            None
        };

        MetricSeries {
            name: MetricName::arbitrary(g),
            tags,
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let metric_series = self.clone();

        Box::new(
            metric_series
                .name
                .shrink()
                .map(move |nme| {
                    let mut ms = metric_series.clone();
                    ms.name = nme;
                    ms
                })
                .flat_map(|metric_series| {
                    metric_series.tags.shrink().map(move |tgs| {
                        let mut ms = metric_series.clone();
                        ms.tags = tgs;
                        ms
                    })
                }),
        )
    }
}

impl Arbitrary for MetricName {
    fn arbitrary(g: &mut Gen) -> Self {
        let namespace = if bool::arbitrary(g) {
            Some(String::from(Name::arbitrary(g)))
        } else {
            None
        };

        MetricName {
            name: String::from(Name::arbitrary(g)),
            namespace,
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let metric_name = self.clone();

        Box::new(
            metric_name
                .name
                .shrink()
                .map(move |name| {
                    let mut mn = metric_name.clone();
                    mn.name = name;
                    mn
                })
                .flat_map(|metric_name| {
                    metric_name.namespace.shrink().map(move |namespace| {
                        let mut mn = metric_name.clone();
                        mn.namespace = namespace;
                        mn
                    })
                }),
        )
    }
}

impl Arbitrary for MetricData {
    fn arbitrary(g: &mut Gen) -> Self {
        let dt = if bool::arbitrary(g) {
            Some(datetime(g))
        } else {
            None
        };

        let interval_ms = bool::arbitrary(g)
            .then(|| u32::arbitrary(g))
            .and_then(std::num::NonZeroU32::new);

        MetricData {
            time: MetricTime {
                timestamp: dt,
                interval_ms,
            },
            kind: MetricKind::arbitrary(g),
            value: MetricValue::arbitrary(g),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let metric_data = self.clone();

        Box::new(
            metric_data
                .kind
                .shrink()
                .map(move |kind| {
                    let mut md = metric_data.clone();
                    md.kind = kind;
                    md
                })
                .flat_map(|metric_data| {
                    metric_data.value.shrink().map(move |value| {
                        let mut md = metric_data.clone();
                        md.value = value;
                        md
                    })
                }),
        )
    }
}

impl Arbitrary for EventMetadata {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut metadata = EventMetadata::default();
        *metadata.value_mut() = Value::arbitrary(g);
        metadata
    }
}
