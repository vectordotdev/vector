use std::{collections::HashMap, error, fmt, iter, num};

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;

use crate::event::metric::{Metric, MetricKind, MetricTags, MetricValue};

static SCOREBOARD: Lazy<HashMap<char, &'static str>> = Lazy::new(|| {
    vec![
        ('_', "waiting"),
        ('S', "starting"),
        ('R', "reading"),
        ('W', "sending"),
        ('K', "keepalive"),
        ('D', "dnslookup"),
        ('C', "closing"),
        ('L', "logging"),
        ('G', "finishing"),
        ('I', "idle_cleanup"),
        ('.', "open"),
    ]
    .into_iter()
    .collect()
});

/// enum of mod_status fields we care about
enum StatusFieldStatistic<'a> {
    ServerUptimeSeconds(u64),
    TotalAccesses(u64),
    TotalKBytes(u64),
    TotalDuration(u64),
    CpuUser(f64),
    CpuSystem(f64),
    CpuChildrenUser(f64),
    CpuChildrenSystem(f64),
    CpuLoad(f64),
    IdleWorkers(u64),
    BusyWorkers(u64),
    ConnsTotal(u64),
    ConnsAsyncWriting(u64),
    ConnsAsyncKeepAlive(u64),
    ConnsAsyncClosing(u64),
    Scoreboard(&'a str),
}

impl<'a> StatusFieldStatistic<'a> {
    fn from_key_value(
        key: &str,
        value: &'a str,
    ) -> Option<Result<StatusFieldStatistic<'a>, ParseError>> {
        match key {
            "ServerUptimeSeconds" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::ServerUptimeSeconds))
            }
            "Total Accesses" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::TotalAccesses))
            }
            "Total kBytes" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::TotalKBytes))
            }
            "Total Duration" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::TotalDuration))
            }
            "CPUUser" => Some(parse_numeric_value(key, value).map(StatusFieldStatistic::CpuUser)),
            "CPUSystem" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::CpuSystem))
            }
            "CPUChildrenUser" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::CpuChildrenUser))
            }
            "CPUChildrenSystem" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::CpuChildrenSystem))
            }
            "CPULoad" => Some(parse_numeric_value(key, value).map(StatusFieldStatistic::CpuLoad)),
            "IdleWorkers" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::IdleWorkers))
            }
            "BusyWorkers" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::BusyWorkers))
            }
            "ConnsTotal" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::ConnsTotal))
            }
            "ConnsAsyncWriting" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::ConnsAsyncWriting))
            }
            "ConnsAsyncClosing" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::ConnsAsyncClosing))
            }
            "ConnsAsyncKeepAlive" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::ConnsAsyncKeepAlive))
            }
            "Scoreboard" => Some(Ok(StatusFieldStatistic::Scoreboard(value))),

            _ => None,
        }
    }
}

/// Parses the text output from Apache's mod_status and returns:
///
/// - A list of metrics generated from the output
/// - A list of parse errors that were encountered
///
/// # Arguments
///
/// - `payload` - the mod_status output
/// - `namespace` - the namespace to put the generated metrics in
/// - `now` - the time the payload was fetched
/// - `tags` - any base tags to apply to the metrics
pub fn parse(
    payload: &str,
    namespace: Option<&str>,
    now: DateTime<Utc>,
    tags: Option<&MetricTags>,
) -> impl Iterator<Item = Result<Metric, ParseError>> {
    // We use a HashMap rather than a Vector as mod_status has
    // BusyWorkers/IdleWorkers repeated
    // https://bz.apache.org/bugzilla/show_bug.cgi?id=63300
    let parsed = payload
        .lines()
        .filter_map(|l| {
            let mut parts = l.splitn(2, ':');
            let key = parts.next();
            let value = parts.next();
            match (key, value) {
                (Some(k), Some(v)) => Some((k, v.trim())),
                _ => None,
            }
        })
        .collect::<HashMap<_, _>>();

    parsed
        .iter()
        .filter_map(|(key, value)| line_to_metrics(key, value, namespace, now, tags))
        .fold(vec![], |mut acc, v| {
            match v {
                Ok(metrics) => metrics.for_each(|v| acc.push(Ok(v))),
                Err(error) => acc.push(Err(error)),
            };
            acc
        })
        .into_iter()
}

fn line_to_metrics<'a>(
    key: &str,
    value: &str,
    namespace: Option<&'a str>,
    now: DateTime<Utc>,
    tags: Option<&'a MetricTags>,
) -> Option<Result<Box<dyn Iterator<Item = Metric> + 'a>, ParseError>> {
    StatusFieldStatistic::from_key_value(key, value).map(move |result| {
        result.map(move |statistic| match statistic {
            StatusFieldStatistic::ServerUptimeSeconds(value) => Box::new(iter::once(
                Metric::new(
                    "uptime_seconds_total",
                    MetricKind::Absolute,
                    MetricValue::Counter {
                        value: value as f64,
                    },
                )
                .with_namespace(namespace.map(str::to_string))
                .with_tags(tags.cloned())
                .with_timestamp(Some(now)),
            )),
            StatusFieldStatistic::TotalAccesses(value) => Box::new(iter::once(
                Metric::new(
                    "access_total",
                    MetricKind::Absolute,
                    MetricValue::Counter {
                        value: value as f64,
                    },
                )
                .with_namespace(namespace.map(str::to_string))
                .with_tags(tags.cloned())
                .with_timestamp(Some(now)),
            )),
            StatusFieldStatistic::TotalKBytes(value) => Box::new(iter::once(
                Metric::new(
                    "sent_bytes_total",
                    MetricKind::Absolute,
                    MetricValue::Counter {
                        value: (value * 1024) as f64,
                    },
                )
                .with_namespace(namespace.map(str::to_string))
                .with_tags(tags.cloned())
                .with_timestamp(Some(now)),
            )),
            StatusFieldStatistic::TotalDuration(value) => Box::new(iter::once(
                Metric::new(
                    "duration_seconds_total",
                    MetricKind::Absolute,
                    MetricValue::Counter {
                        value: value as f64,
                    },
                )
                .with_namespace(namespace.map(str::to_string))
                .with_tags(tags.cloned())
                .with_timestamp(Some(now)),
            )),
            StatusFieldStatistic::CpuUser(value) => Box::new(iter::once(
                Metric::new(
                    "cpu_seconds_total",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value },
                )
                .with_namespace(namespace.map(str::to_string))
                .with_tags({
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.replace("type".to_string(), "user".to_string());
                    Some(tags)
                })
                .with_timestamp(Some(now)),
            ))
                as Box<dyn Iterator<Item = Metric>>,
            StatusFieldStatistic::CpuSystem(value) => Box::new(iter::once(
                Metric::new(
                    "cpu_seconds_total",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value },
                )
                .with_namespace(namespace.map(str::to_string))
                .with_tags({
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.replace("type".to_string(), "system".to_string());
                    Some(tags)
                })
                .with_timestamp(Some(now)),
            ))
                as Box<dyn Iterator<Item = Metric>>,
            StatusFieldStatistic::CpuChildrenUser(value) => Box::new(iter::once(
                Metric::new(
                    "cpu_seconds_total",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value },
                )
                .with_namespace(namespace.map(str::to_string))
                .with_tags({
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.replace("type".to_string(), "children_user".to_string());
                    Some(tags)
                })
                .with_timestamp(Some(now)),
            ))
                as Box<dyn Iterator<Item = Metric>>,
            StatusFieldStatistic::CpuChildrenSystem(value) => Box::new(iter::once(
                Metric::new(
                    "cpu_seconds_total",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value },
                )
                .with_namespace(namespace.map(str::to_string))
                .with_tags({
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.replace("type".to_string(), "children_system".to_string());
                    Some(tags)
                })
                .with_timestamp(Some(now)),
            ))
                as Box<dyn Iterator<Item = Metric>>,
            StatusFieldStatistic::CpuLoad(value) => Box::new(iter::once(
                Metric::new(
                    "cpu_load",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value },
                )
                .with_namespace(namespace.map(str::to_string))
                .with_tags(tags.cloned())
                .with_timestamp(Some(now)),
            ))
                as Box<dyn Iterator<Item = Metric>>,
            StatusFieldStatistic::IdleWorkers(value) => Box::new(iter::once(
                Metric::new(
                    "workers",
                    MetricKind::Absolute,
                    MetricValue::Gauge {
                        value: value as f64,
                    },
                )
                .with_namespace(namespace.map(str::to_string))
                .with_tags({
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.replace("state".to_string(), "idle".to_string());
                    Some(tags)
                })
                .with_timestamp(Some(now)),
            ))
                as Box<dyn Iterator<Item = Metric>>,
            StatusFieldStatistic::BusyWorkers(value) => Box::new(iter::once(
                Metric::new(
                    "workers",
                    MetricKind::Absolute,
                    MetricValue::Gauge {
                        value: value as f64,
                    },
                )
                .with_namespace(namespace.map(str::to_string))
                .with_tags({
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.replace("state".to_string(), "busy".to_string());
                    Some(tags)
                })
                .with_timestamp(Some(now)),
            )),
            StatusFieldStatistic::ConnsTotal(value) => Box::new(iter::once(
                Metric::new(
                    "connections",
                    MetricKind::Absolute,
                    MetricValue::Gauge {
                        value: value as f64,
                    },
                )
                .with_namespace(namespace.map(str::to_string))
                .with_tags({
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.replace("state".to_string(), "total".to_string());
                    Some(tags)
                })
                .with_timestamp(Some(now)),
            )),
            StatusFieldStatistic::ConnsAsyncWriting(value) => Box::new(iter::once(
                Metric::new(
                    "connections",
                    MetricKind::Absolute,
                    MetricValue::Gauge {
                        value: value as f64,
                    },
                )
                .with_namespace(namespace.map(str::to_string))
                .with_tags({
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.replace("state".to_string(), "writing".to_string());
                    Some(tags)
                })
                .with_timestamp(Some(now)),
            )),
            StatusFieldStatistic::ConnsAsyncClosing(value) => Box::new(iter::once(
                Metric::new(
                    "connections",
                    MetricKind::Absolute,
                    MetricValue::Gauge {
                        value: value as f64,
                    },
                )
                .with_namespace(namespace.map(str::to_string))
                .with_tags({
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.replace("state".to_string(), "closing".to_string());
                    Some(tags)
                })
                .with_timestamp(Some(now)),
            )),
            StatusFieldStatistic::ConnsAsyncKeepAlive(value) => Box::new(iter::once(
                Metric::new(
                    "connections",
                    MetricKind::Absolute,
                    MetricValue::Gauge {
                        value: value as f64,
                    },
                )
                .with_namespace(namespace.map(str::to_string))
                .with_tags({
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.replace("state".to_string(), "keepalive".to_string());
                    Some(tags)
                })
                .with_timestamp(Some(now)),
            )),
            StatusFieldStatistic::Scoreboard(value) => {
                let scores = value.chars().fold(HashMap::new(), |mut m, c| {
                    *m.entry(c).or_insert(0u32) += 1;
                    m
                });

                Box::new(SCOREBOARD.iter().map(move |(c, name)| {
                    score_to_metric(
                        namespace,
                        now,
                        tags,
                        name,
                        scores.get(c).copied().unwrap_or_default(),
                    )
                })) as Box<dyn Iterator<Item = Metric>>
            }
        })
    })
}

fn parse_numeric_value<T: std::str::FromStr>(key: &str, value: &str) -> Result<T, ParseError>
where
    T::Err: Into<ValueParseError> + 'static,
{
    value.parse::<T>().map_err(|error| ParseError {
        key: key.to_string(),
        error: error.into(),
    })
}

fn score_to_metric(
    namespace: Option<&str>,
    now: DateTime<Utc>,
    tags: Option<&MetricTags>,
    state: &str,
    count: u32,
) -> Metric {
    Metric::new(
        "scoreboard",
        MetricKind::Absolute,
        MetricValue::Gauge {
            value: count.into(),
        },
    )
    .with_namespace(namespace.map(str::to_string))
    .with_tags({
        let mut tags = tags.cloned().unwrap_or_default();
        tags.replace("state".to_string(), state.to_string());
        Some(tags)
    })
    .with_timestamp(Some(now))
}

#[derive(Debug)]
enum ValueParseError {
    Float(num::ParseFloatError),
    Int(num::ParseIntError),
}

impl From<num::ParseFloatError> for ValueParseError {
    fn from(error: num::ParseFloatError) -> ValueParseError {
        ValueParseError::Float(error)
    }
}

impl From<num::ParseIntError> for ValueParseError {
    fn from(error: num::ParseIntError) -> ValueParseError {
        ValueParseError::Int(error)
    }
}

impl error::Error for ValueParseError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            ValueParseError::Float(ref e) => Some(e),
            ValueParseError::Int(ref e) => Some(e),
        }
    }
}

impl fmt::Display for ValueParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ValueParseError::Float(ref e) => e.fmt(f),
            ValueParseError::Int(ref e) => e.fmt(f),
        }
    }
}

#[derive(Debug)]
pub struct ParseError {
    key: String,
    error: ValueParseError,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "could not parse value for {}: {}", self.key, self.error)
    }
}

impl error::Error for ParseError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        Some(&self.error)
    }
}

#[cfg(test)]
mod test {
    use chrono::{DateTime, Utc};
    use similar_asserts::assert_eq;
    use vector_lib::assert_event_data_eq;
    use vector_lib::metric_tags;

    use super::*;
    use crate::event::metric::{Metric, MetricKind, MetricValue};

    // Test ExtendedStatus: Off
    // https://httpd.apache.org/docs/2.4/mod/core.html#extendedstatus
    #[test]
    fn test_not_extended() {
        let payload = r##"
localhost
ServerVersion: Apache/2.4.46 (Unix)
ServerMPM: event
Server Built: Aug  5 2020 23:20:17
CurrentTime: Thursday, 03-Sep-2020 20:48:54 UTC
RestartTime: Thursday, 03-Sep-2020 20:48:41 UTC
ParentServerConfigGeneration: 1
ParentServerMPMGeneration: 0
ServerUptimeSeconds: 12
ServerUptime: 12 seconds
Load1: 0.75
Load5: 0.59
Load15: 0.76
BusyWorkers: 1
IdleWorkers: 74
Processes: 3
Stopping: 0
BusyWorkers: 1
IdleWorkers: 74
ConnsTotal: 1
ConnsAsyncWriting: 0
ConnsAsyncKeepAlive: 0
ConnsAsyncClosing: 0
Scoreboard: ____S_____I______R____I_______KK___D__C__G_L____________W__________________.....................................................................................................................................................................................................................................................................................................................................
            "##;

        let (now, metrics, errors) = parse_sort(payload);

        assert_event_data_eq!(
            metrics,
            vec![
                Metric::new(
                    "connections",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 0.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "closing")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "connections",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 0.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "keepalive")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "connections",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "total")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "connections",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 0.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "writing")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "closing")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "dnslookup")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "finishing")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 2.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "idle_cleanup")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 2.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "keepalive")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "logging")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 325.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "open")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "reading")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "sending")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "starting")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 64.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "waiting")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "uptime_seconds_total",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 12.0 },
                )
                .with_namespace(Some("apache"))
                .with_timestamp(Some(now)),
                Metric::new(
                    "workers",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "busy")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "workers",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 74.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "idle")))
                .with_timestamp(Some(now)),
            ]
        );
        assert_eq!(errors.len(), 0);
    }

    // Test ExtendedStatus: On
    // https://httpd.apache.org/docs/2.4/mod/core.html#extendedstatus
    #[test]
    fn test_extended() {
        let payload = r##"
localhost
ServerVersion: Apache/2.4.46 (Unix)
ServerMPM: event
Server Built: Aug  5 2020 23:20:17
CurrentTime: Friday, 21-Aug-2020 18:41:34 UTC
RestartTime: Friday, 21-Aug-2020 18:41:08 UTC
ParentServerConfigGeneration: 1
ParentServerMPMGeneration: 0
ServerUptimeSeconds: 26
ServerUptime: 26 seconds
Load1: 0.00
Load5: 0.03
Load15: 0.03
Total Accesses: 30
Total kBytes: 217
Total Duration: 11
CPUUser: .2
CPUSystem: .02
CPUChildrenUser: 0
CPUChildrenSystem: 0
CPULoad: .846154
Uptime: 26
ReqPerSec: 1.15385
BytesPerSec: 8546.46
BytesPerReq: 7406.93
DurationPerReq: .366667
BusyWorkers: 1
IdleWorkers: 74
Processes: 3
Stopping: 0
BusyWorkers: 1
IdleWorkers: 74
ConnsTotal: 1
ConnsAsyncWriting: 0
ConnsAsyncKeepAlive: 0
ConnsAsyncClosing: 0
Scoreboard: ____S_____I______R____I_______KK___D__C__G_L____________W__________________.....................................................................................................................................................................................................................................................................................................................................
            "##;

        let (now, metrics, errors) = parse_sort(payload);

        assert_event_data_eq!(
            metrics,
            vec![
                Metric::new(
                    "access_total",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 30.0 },
                )
                .with_namespace(Some("apache"))
                .with_timestamp(Some(now)),
                Metric::new(
                    "connections",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 0.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "closing")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "connections",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 0.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "keepalive")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "connections",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "total")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "connections",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 0.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "writing")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "cpu_load",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 0.846154 },
                )
                .with_namespace(Some("apache"))
                .with_timestamp(Some(now)),
                Metric::new(
                    "cpu_seconds_total",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 0.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("type" => "children_system")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "cpu_seconds_total",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 0.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("type" => "children_user")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "cpu_seconds_total",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 0.02 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("type" => "system")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "cpu_seconds_total",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 0.2 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("type" => "user")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "duration_seconds_total",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 11.0 },
                )
                .with_namespace(Some("apache"))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "closing")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "dnslookup")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "finishing")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 2.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "idle_cleanup")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 2.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "keepalive")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "logging")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 325.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "open")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "reading")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "sending")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "starting")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "scoreboard",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 64.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "waiting")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "sent_bytes_total",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 222208.0 },
                )
                .with_namespace(Some("apache"))
                .with_timestamp(Some(now)),
                Metric::new(
                    "uptime_seconds_total",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 26.0 },
                )
                .with_namespace(Some("apache"))
                .with_timestamp(Some(now)),
                Metric::new(
                    "workers",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "busy")))
                .with_timestamp(Some(now)),
                Metric::new(
                    "workers",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 74.0 },
                )
                .with_namespace(Some("apache"))
                .with_tags(Some(metric_tags!("state" => "idle")))
                .with_timestamp(Some(now)),
            ]
        );
        assert_eq!(errors.len(), 0);
    }

    #[test]
    fn test_parse_failure() {
        let payload = r##"
ServerUptimeSeconds: not a number
ConnsTotal: 1
            "##;

        let (now, metrics, errors) = parse_sort(payload);

        assert_event_data_eq!(
            metrics,
            vec![Metric::new(
                "connections",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 1.0 },
            )
            .with_namespace(Some("apache"))
            .with_tags(Some(metric_tags!("state" => "total")))
            .with_timestamp(Some(now)),]
        );
        assert_eq!(errors.len(), 1);
    }

    fn parse_sort(payload: &str) -> (DateTime<Utc>, Vec<Metric>, Vec<ParseError>) {
        let now: DateTime<Utc> = Utc::now();
        let (mut metrics, errors) = parse(payload, Some("apache"), now, None).fold(
            (vec![], vec![]),
            |(mut metrics, mut errors), v| {
                match v {
                    Ok(m) => metrics.push(m),
                    Err(e) => errors.push(e),
                }
                (metrics, errors)
            },
        );

        metrics.sort_by_key(|metric| metric.series().to_string());

        (now, metrics, errors)
    }
}
