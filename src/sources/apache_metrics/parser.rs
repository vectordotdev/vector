use crate::event::metric::{Metric, MetricKind, MetricValue};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;

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
) -> (Vec<Metric>, Vec<ParseError>) {
    let mut parsed = payload
        .lines()
        .into_iter()
        .filter_map(|l| {
            let mut parts = l.splitn(2, ":");
            let key = parts.next();
            let value = parts.next().map(|s| s.trim());
            match (key, value) {
                (Some(k), Some(v)) => Some((k, v)),
                _ => None,
            }
        })
        .collect::<Vec<_>>();

    // mod_status has BusyWorkers/IdleWorkers repeated
    // TODO better way to do this without .collect()ing? Do we care?
    parsed.sort();
    parsed.dedup();

    parsed
        .iter()
        .map(|(key, value)| line_to_metrics(key, value, namespace, now, tags))
        .fold(
            (Vec::new(), Vec::new()),
            |(mut metrics, mut errs), current| {
                match current {
                    LineResult::Metrics(m) => metrics.extend(m),
                    LineResult::Error(err) => errs.push(err),
                    LineResult::None => {}
                }
                (metrics, errs)
            },
        )
}

#[derive(Debug)]
pub struct ParseError {
    key: String,
    err: Box<dyn Error>,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "could not parse value for {}: {}", self.key, self.err)
    }
}

impl Error for ParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.err.as_ref())
    }
}

enum LineResult {
    Metrics(Vec<Metric>),
    Error(ParseError),
    None,
}

fn line_to_metrics(
    key: &str,
    value: &str,
    namespace: &str,
    now: DateTime<Utc>,
    tags: Option<&BTreeMap<String, String>>,
) -> LineResult {
    match key {
        "ServerUptimeSeconds" => match value.parse::<f64>() {
            Ok(value) => LineResult::Metrics(vec![Metric {
                name: encode_namespace(namespace, "uptime_seconds_total"),
                timestamp: Some(now),
                tags: tags.map(|tags| tags.clone()),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value },
            }]),
            Err(err) => LineResult::Error(ParseError {
                key: key.to_string(),
                err: err.into(),
            }),
        },
        "Total Accesses" => match value.parse::<f64>() {
            Ok(value) => LineResult::Metrics(vec![Metric {
                name: encode_namespace(namespace, "access_total"),
                timestamp: Some(now),
                tags: tags.map(|tags| tags.clone()),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value },
            }]),
            Err(err) => LineResult::Error(ParseError {
                key: key.to_string(),
                err: err.into(),
            }),
        },
        "Total kBytes" => match value.parse::<u32>().map(|v| v * 1024) {
            Ok(value) => LineResult::Metrics(vec![Metric {
                name: encode_namespace(namespace, "sent_bytes_total"),
                timestamp: Some(now),
                tags: tags.map(|tags| tags.clone()),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter {
                    value: value.into(),
                },
            }]),
            Err(err) => LineResult::Error(ParseError {
                key: key.to_string(),
                err: err.into(),
            }),
        },
        "Total Duration" => match value.parse::<f64>() {
            Ok(value) => LineResult::Metrics(vec![Metric {
                name: encode_namespace(namespace, "duration_seconds_total"),
                timestamp: Some(now),
                tags: tags.map(|tags| tags.clone()),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value }, // TODO verify unit
            }]),
            Err(err) => LineResult::Error(ParseError {
                key: key.to_string(),
                err: err.into(),
            }),
        },
        "CPUUser" => match value.parse::<f64>() {
            Ok(value) => LineResult::Metrics(vec![Metric {
                name: encode_namespace(namespace, "cpu_seconds_total"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.map(|tags| tags.clone()).unwrap_or_default();
                    tags.insert("type".to_string(), "user".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }]),
            Err(err) => LineResult::Error(ParseError {
                key: key.to_string(),
                err: err.into(),
            }),
        },
        "CPUSystem" => match value.parse::<f64>() {
            Ok(value) => LineResult::Metrics(vec![Metric {
                name: encode_namespace(namespace, "cpu_seconds_total"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.map(|tags| tags.clone()).unwrap_or_default();
                    tags.insert("type".to_string(), "system".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }]),
            Err(err) => LineResult::Error(ParseError {
                key: key.to_string(),
                err: err.into(),
            }),
        },
        "CPUChildrenUser" => match value.parse::<f64>() {
            Ok(value) => LineResult::Metrics(vec![Metric {
                name: encode_namespace(namespace, "cpu_seconds_total"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.map(|tags| tags.clone()).unwrap_or_default();
                    tags.insert("type".to_string(), "children_user".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }]),
            Err(err) => LineResult::Error(ParseError {
                key: key.to_string(),
                err: err.into(),
            }),
        },
        "CPUChildrenSystem" => match value.parse::<f64>() {
            Ok(value) => LineResult::Metrics(vec![Metric {
                name: encode_namespace(namespace, "cpu_seconds_total"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.map(|tags| tags.clone()).unwrap_or_default();
                    tags.insert("type".to_string(), "children_system".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }]),
            Err(err) => LineResult::Error(ParseError {
                key: key.to_string(),
                err: err.into(),
            }),
        },
        "CPULoad" => match value.parse::<f64>() {
            Ok(value) => LineResult::Metrics(vec![Metric {
                name: encode_namespace(namespace, "cpu_load"),
                timestamp: Some(now),
                tags: tags.map(|tags| tags.clone()),
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }]),
            Err(err) => LineResult::Error(ParseError {
                key: key.to_string(),
                err: err.into(),
            }),
        },
        "IdleWorkers" => match value.parse::<f64>() {
            Ok(value) => LineResult::Metrics(vec![Metric {
                name: encode_namespace(namespace, "workers"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.map(|tags| tags.clone()).unwrap_or_default();
                    tags.insert("state".to_string(), "idle".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }]),
            Err(err) => LineResult::Error(ParseError {
                key: key.to_string(),
                err: err.into(),
            }),
        },
        "BusyWorkers" => match value.parse::<f64>() {
            Ok(value) => LineResult::Metrics(vec![Metric {
                name: encode_namespace(namespace, "workers"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.map(|tags| tags.clone()).unwrap_or_default();
                    tags.insert("state".to_string(), "busy".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }]),
            Err(err) => LineResult::Error(ParseError {
                key: key.to_string(),
                err: err.into(),
            }),
        },
        "ConnsTotal" => match value.parse::<f64>() {
            Ok(value) => LineResult::Metrics(vec![Metric {
                name: encode_namespace(namespace, "connections"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.map(|tags| tags.clone()).unwrap_or_default();
                    tags.insert("state".to_string(), "total".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }]),
            Err(err) => LineResult::Error(ParseError {
                key: key.to_string(),
                err: err.into(),
            }),
        },
        "ConnsAsyncWriting" => match value.parse::<f64>() {
            Ok(value) => LineResult::Metrics(vec![Metric {
                name: encode_namespace(namespace, "connections"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.map(|tags| tags.clone()).unwrap_or_default();
                    tags.insert("state".to_string(), "writing".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }]),
            Err(err) => LineResult::Error(ParseError {
                key: key.to_string(),
                err: err.into(),
            }),
        },
        "ConnsAsyncClosing" => match value.parse::<f64>() {
            Ok(value) => LineResult::Metrics(vec![Metric {
                name: encode_namespace(namespace, "connections"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.map(|tags| tags.clone()).unwrap_or_default();
                    tags.insert("state".to_string(), "closing".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }]),
            Err(err) => LineResult::Error(ParseError {
                key: key.to_string(),
                err: err.into(),
            }),
        },
        "ConnsAsyncKeepAlive" => match value.parse::<f64>() {
            Ok(value) => LineResult::Metrics(vec![Metric {
                name: encode_namespace(namespace, "connections"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.map(|tags| tags.clone()).unwrap_or_default();
                    tags.insert("state".to_string(), "keepalive".to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value },
            }]),
            Err(err) => LineResult::Error(ParseError {
                key: key.to_string(),
                err: err.into(),
            }),
        },
        "Scoreboard" => {
            let to_metric = |state: &str, count: &u32| Metric {
                name: encode_namespace(namespace, "scoreboard"),
                timestamp: Some(now),
                tags: {
                    let mut tags = tags.map(|tags| tags.clone()).unwrap_or_default();
                    tags.insert("state".to_string(), state.to_string());
                    Some(tags)
                },
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge {
                    value: (*count).into(),
                },
            };

            let scores = value.chars().fold(HashMap::new(), |mut m, c| {
                *m.entry(c).or_insert(0u32) += 1;
                m
            });

            let scoreboard: HashMap<char, &str> = vec![
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

            LineResult::Metrics(
                scoreboard
                    .iter()
                    .map(|(c, name)| to_metric(name, scores.get(c).unwrap_or(&0u32)))
                    .collect::<Vec<_>>(),
            )
        }
        _ => LineResult::None,
    }
}

fn encode_namespace(namespace: &str, name: &str) -> String {
    if !namespace.is_empty() {
        format!("{}_{}", namespace, name)
    } else {
        name.to_string()
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

        let (mut metrics, errors) = parse(payload, "apache", now, None);
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

        let (mut metrics, errors) = parse(payload, "apache", now, None);
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

        let (mut metrics, errors) = parse(payload, "apache", now, None);
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
