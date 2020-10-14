use crate::event::metric::{Metric, MetricKind, MetricValue};
use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::{error, fmt, iter, num};

lazy_static! {
    static ref SCOREBOARD: HashMap<char, &'static str> = vec![
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
    .collect();
}

/// enum of mod_status fields we care about
enum StatusFieldStatistic<'a> {
    ServerUptimeSeconds(u64),
    TotalAccesses(u64),
    TotalKBytes(u64),
    TotalDuration(u64),
    CPUUser(f64),
    CPUSystem(f64),
    CPUChildrenUser(f64),
    CPUChildrenSystem(f64),
    CPULoad(f64),
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
            "CPUUser" => Some(parse_numeric_value(key, value).map(StatusFieldStatistic::CPUUser)),
            "CPUSystem" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::CPUSystem))
            }
            "CPUChildrenUser" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::CPUChildrenUser))
            }
            "CPUChildrenSystem" => {
                Some(parse_numeric_value(key, value).map(StatusFieldStatistic::CPUChildrenSystem))
            }
            "CPULoad" => Some(parse_numeric_value(key, value).map(StatusFieldStatistic::CPULoad)),
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
    namespace: &str,
    now: DateTime<Utc>,
    tags: Option<&BTreeMap<String, String>>,
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
                Err(err) => acc.push(Err(err)),
            };
            acc
        })
        .into_iter()
}

pub fn encode_namespace(namespace: &str, name: &str) -> String {
    match namespace {
        "" => name.to_string(),
        _ => [namespace, name].join("_"),
    }
}

fn line_to_metrics<'a>(
    key: &str,
    value: &str,
    namespace: &'a str,
    now: DateTime<Utc>,
    tags: Option<&'a BTreeMap<String, String>>,
) -> Option<Result<Box<dyn Iterator<Item = Metric> + 'a>, ParseError>> {
    StatusFieldStatistic::from_key_value(key, value).map(move |result| {
        result.map(move |statistic| match statistic {
            StatusFieldStatistic::ServerUptimeSeconds(value) => Box::new(iter::once(Metric {
                name: encode_namespace(namespace, "uptime_seconds_total"),
                timestamp: Some(now),
                tags: tags.cloned(),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter {
                    value: value as f64,
                },
            })),
            StatusFieldStatistic::TotalAccesses(value) => Box::new(iter::once(Metric {
                name: encode_namespace(namespace, "access_total"),
                timestamp: Some(now),
                tags: tags.cloned(),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter {
                    value: value as f64,
                },
            })),
            StatusFieldStatistic::TotalKBytes(value) => Box::new(iter::once(Metric {
                name: encode_namespace(namespace, "sent_bytes_total"),
                timestamp: Some(now),
                tags: tags.cloned(),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter {
                    value: (value * 1024) as f64,
                },
            })),
            StatusFieldStatistic::TotalDuration(value) => Box::new(iter::once(Metric {
                name: encode_namespace(namespace, "duration_seconds_total"),
                timestamp: Some(now),
                tags: tags.cloned(),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter {
                    value: value as f64,
                },
            })),
            StatusFieldStatistic::CPUUser(value) => Box::new(iter::once(Metric {
                name: encode_namespace(namespace, "cpu_seconds_total"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.insert("type".to_string(), "user".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }))
                as Box<dyn Iterator<Item = Metric>>,
            StatusFieldStatistic::CPUSystem(value) => Box::new(iter::once(Metric {
                name: encode_namespace(namespace, "cpu_seconds_total"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.insert("type".to_string(), "system".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }))
                as Box<dyn Iterator<Item = Metric>>,
            StatusFieldStatistic::CPUChildrenUser(value) => Box::new(iter::once(Metric {
                name: encode_namespace(namespace, "cpu_seconds_total"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.insert("type".to_string(), "children_user".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }))
                as Box<dyn Iterator<Item = Metric>>,
            StatusFieldStatistic::CPUChildrenSystem(value) => Box::new(iter::once(Metric {
                name: encode_namespace(namespace, "cpu_seconds_total"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.insert("type".to_string(), "children_system".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }))
                as Box<dyn Iterator<Item = Metric>>,
            StatusFieldStatistic::CPULoad(value) => Box::new(iter::once(Metric {
                name: encode_namespace(namespace, "cpu_load"),
                timestamp: Some(now),
                tags: tags.cloned(),
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }))
                as Box<dyn Iterator<Item = Metric>>,
            StatusFieldStatistic::IdleWorkers(value) => Box::new(iter::once(Metric {
                name: encode_namespace(namespace, "workers"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.insert("state".to_string(), "idle".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge {
                    value: value as f64,
                },
            }))
                as Box<dyn Iterator<Item = Metric>>,
            StatusFieldStatistic::BusyWorkers(value) => Box::new(iter::once(Metric {
                name: encode_namespace(namespace, "workers"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.insert("state".to_string(), "busy".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge {
                    value: value as f64,
                },
            })),
            StatusFieldStatistic::ConnsTotal(value) => Box::new(iter::once(Metric {
                name: encode_namespace(namespace, "connections"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.insert("state".to_string(), "total".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge {
                    value: value as f64,
                },
            })),
            StatusFieldStatistic::ConnsAsyncWriting(value) => Box::new(iter::once(Metric {
                name: encode_namespace(namespace, "connections"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.insert("state".to_string(), "writing".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge {
                    value: value as f64,
                },
            })),
            StatusFieldStatistic::ConnsAsyncClosing(value) => Box::new(iter::once(Metric {
                name: encode_namespace(namespace, "connections"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.insert("state".to_string(), "closing".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge {
                    value: value as f64,
                },
            })),
            StatusFieldStatistic::ConnsAsyncKeepAlive(value) => Box::new(iter::once(Metric {
                name: encode_namespace(namespace, "connections"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.cloned().unwrap_or_default();
                    tags.insert("state".to_string(), "keepalive".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge {
                    value: value as f64,
                },
            })),
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
    value.parse::<T>().map_err(|err| ParseError {
        key: key.to_string(),
        err: err.into(),
    })
}

fn score_to_metric(
    namespace: &str,
    now: DateTime<Utc>,
    tags: Option<&BTreeMap<String, String>>,
    state: &str,
    count: u32,
) -> Metric {
    Metric {
        name: encode_namespace(namespace, "scoreboard"),
        timestamp: Some(now),
        tags: {
            let mut tags = tags.cloned().unwrap_or_default();
            tags.insert("state".to_string(), state.to_string());
            Some(tags)
        },
        kind: MetricKind::Absolute,
        value: MetricValue::Gauge {
            value: count.into(),
        },
    }
}

#[derive(Debug)]
enum ValueParseError {
    Float(num::ParseFloatError),
    Int(num::ParseIntError),
}

impl From<num::ParseFloatError> for ValueParseError {
    fn from(err: num::ParseFloatError) -> ValueParseError {
        ValueParseError::Float(err)
    }
}

impl From<num::ParseIntError> for ValueParseError {
    fn from(err: num::ParseIntError) -> ValueParseError {
        ValueParseError::Int(err)
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
    err: ValueParseError,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "could not parse value for {}: {}", self.key, self.err)
    }
}

impl error::Error for ParseError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        Some(&self.err)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::event::metric::{Metric, MetricKind, MetricValue};
    use chrono::{DateTime, Utc};
    use pretty_assertions::assert_eq;
    use std::collections::BTreeMap;

    macro_rules! map {
        ($($key:expr => $value:expr),*) => {
            {
                let mut m = BTreeMap::new();
                $(
                    m.insert($key.into(), $value.into());
                )*
                m
            }
        };
    }

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

        let now: DateTime<Utc> = Utc::now();

        let (mut metrics, errors) = parse(payload, "apache", now, None).fold(
            (vec![], vec![]),
            |(mut metrics, mut errors), v| {
                match v {
                    Ok(m) => metrics.push(m),
                    Err(e) => errors.push(e),
                }
                (metrics, errors)
            },
        );
        metrics.sort_by(|a, b| (&a.name, &a.tags).cmp(&(&b.name, &b.tags)));

        assert_eq!(
            metrics,
            vec![
                Metric {
                    name: "apache_connections".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "closing"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 0.0 },
                },
                Metric {
                    name: "apache_connections".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "keepalive"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 0.0 },
                },
                Metric {
                    name: "apache_connections".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "total"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_connections".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "writing"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 0.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "closing"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "dnslookup"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "finishing"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "idle_cleanup"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 2.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "keepalive"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 2.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "logging"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "open"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 325.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "reading"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "sending"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "starting"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "waiting"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 64.0 },
                },
                Metric {
                    name: "apache_uptime_seconds_total".into(),
                    timestamp: Some(now),
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 12.0 },
                },
                Metric {
                    name: "apache_workers".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "busy"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_workers".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "idle"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 74.0 },
                },
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

        let now: DateTime<Utc> = Utc::now();

        let (mut metrics, errors) = parse(payload, "apache", now, None).fold(
            (vec![], vec![]),
            |(mut metrics, mut errors), v| {
                match v {
                    Ok(m) => metrics.push(m),
                    Err(e) => errors.push(e),
                }
                (metrics, errors)
            },
        );
        metrics.sort_by(|a, b| (&a.name, &a.tags).cmp(&(&b.name, &b.tags)));

        assert_eq!(
            metrics,
            vec![
                Metric {
                    name: "apache_access_total".into(),
                    timestamp: Some(now),
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 30.0 },
                },
                Metric {
                    name: "apache_connections".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "closing"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 0.0 },
                },
                Metric {
                    name: "apache_connections".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "keepalive"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 0.0 },
                },
                Metric {
                    name: "apache_connections".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "total"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_connections".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "writing"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 0.0 },
                },
                Metric {
                    name: "apache_cpu_load".into(),
                    timestamp: Some(now),
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 0.846154 },
                },
                Metric {
                    name: "apache_cpu_seconds_total".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"type" => "children_system"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 0.0 },
                },
                Metric {
                    name: "apache_cpu_seconds_total".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"type" => "children_user"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 0.0 },
                },
                Metric {
                    name: "apache_cpu_seconds_total".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"type" => "system"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 0.02 },
                },
                Metric {
                    name: "apache_cpu_seconds_total".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"type" => "user"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 0.2 },
                },
                Metric {
                    name: "apache_duration_seconds_total".into(),
                    timestamp: Some(now),
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 11.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "closing"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "dnslookup"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "finishing"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "idle_cleanup"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 2.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "keepalive"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 2.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "logging"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "open"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 325.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "reading"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "sending"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "starting"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_scoreboard".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "waiting"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 64.0 },
                },
                Metric {
                    name: "apache_sent_bytes_total".into(),
                    timestamp: Some(now),
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 222208.0 },
                },
                Metric {
                    name: "apache_uptime_seconds_total".into(),
                    timestamp: Some(now),
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 26.0 },
                },
                Metric {
                    name: "apache_workers".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "busy"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "apache_workers".into(),
                    timestamp: Some(now),
                    tags: Some(map! {"state" => "idle"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 74.0 },
                },
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

        let now: DateTime<Utc> = Utc::now();

        let (mut metrics, errors) = parse(payload, "apache", now, None).fold(
            (vec![], vec![]),
            |(mut metrics, mut errors), v| {
                match v {
                    Ok(m) => metrics.push(m),
                    Err(e) => errors.push(e),
                }
                (metrics, errors)
            },
        );
        metrics.sort_by(|a, b| (&a.name, &a.tags).cmp(&(&b.name, &b.tags)));

        assert_eq!(
            metrics,
            vec![Metric {
                name: "apache_connections".into(),
                timestamp: Some(now),
                tags: Some(map! {"state" => "total"}),
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value: 1.0 },
            },]
        );
        assert_eq!(errors.len(), 1);
    }
}
