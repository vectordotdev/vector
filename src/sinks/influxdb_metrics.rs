use crate::{
    event::metric::{Metric, MetricKind, MetricValue},
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::collections::BTreeMap;

pub enum Field {
    /// string
    String(String),
    /// Float
    Float(f64),
}

fn encode_events(events: Vec<Metric>, namespace: &str) -> Vec<String> {
    events
        .into_iter()
        .filter_map(|event| {
            let fullname = encode_namespace(namespace, &event.name);
            let ts = encode_timestamp(event.timestamp);
            let tags = event.tags.clone();
            match event.value {
                MetricValue::Counter { value } => {
                    let fields = to_fields(value);

                    Some(vec![influx_line_protocol(fullname, "counter", tags, Some(fields), ts)])
                }
                MetricValue::Gauge { value } => {
                    let fields = to_fields(value);

                    Some(vec![influx_line_protocol(fullname, "gauge", tags, Some(fields), ts)])
                }
                MetricValue::Set { values } => {
                    let fields = to_fields(values.len() as f64);

                    Some(vec![influx_line_protocol(fullname, "set", tags, Some(fields), ts)])
                }
                _ => None
            }
        })
        .flatten()
        .collect()
}

fn influx_line_protocol(measurement: String, metric_type: &str, tags: Option<HashMap<String, String>>, fields: Option<HashMap<String, Field>>, timestamp: i64) -> String {
    let mut line_protocol = vec![encode_key(measurement)];

    // Tags
    let mut unwrapped_tags = tags.unwrap_or(HashMap::new());
    unwrapped_tags.insert("metric_type".to_owned(), metric_type.to_owned());
    line_protocol.push(format!(",{}", encode_tags(unwrapped_tags)));

    // Fields
    let unwrapped_fields = fields.unwrap_or(HashMap::new());
    line_protocol.push(format!(" {}", encode_fields(unwrapped_fields)));

    // Timestamp
    line_protocol.push(format!(" {}", timestamp));

    line_protocol.join("")
}

fn encode_key(key: String) -> String {
    key.replace("\\", "\\\\")
        .replace(",", "\\,")
        .replace(" ", "\\ ")
        .replace("=", "\\=")
}

fn encode_tags(tags: HashMap<String, String>) -> String {
    let ordered: Vec<String> = tags
        // sort by key
        .iter().collect::<BTreeMap<_, _>>()
        // map to key=value
        .iter().map(|pair| {
        let key = encode_key(pair.0.to_string());
        let value = encode_key(pair.1.to_string());
        if !key.is_empty() && !value.is_empty() {
            format!("{}={}", key, value)
        } else {
            "".to_string()
        }
    })
        // filter empty
        .filter(|tag_value| !tag_value.is_empty())
        .collect();

    ordered.join(",")
}

fn encode_fields(fields: HashMap<String, Field>) -> String {
    let encoded = fields
        // sort by key
        .iter().collect::<BTreeMap<_, _>>()
        // map to key=value
        .iter().map(|pair| {
        let key = encode_key(pair.0.to_string());
        let value = match pair.1 {
            Field::String(s) => {
                let escaped = s.replace("\\", "\\\\").replace("\"", "\\\"");
                format!("\"{}\"", escaped)
            }
            Field::Float(f) => f.to_string(),
        };
        if !key.is_empty() && !value.is_empty() {
            format!("{}={}", key, value)
        } else {
            "".to_string()
        }
    })
        .filter(|field_value| !field_value.is_empty())
        .collect::<Vec<String>>();

    encoded.join(",")
}

fn encode_timestamp(timestamp: Option<DateTime<Utc>>) -> i64 {
    if let Some(ts) = timestamp {
        ts.timestamp_nanos()
    } else {
        encode_timestamp(Some(Utc::now()))
    }
}

fn encode_namespace(namespace: &str, name: &str) -> String {
    if !namespace.is_empty() {
        format!("{}.{}", namespace, name)
    } else {
        name.to_string()
    }
}

fn to_fields(value: f64) -> HashMap<String, Field> {
    let fields: HashMap<String, Field> = vec![
        ("value".to_owned(), Field::Float(value)),
    ].into_iter().collect();
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::offset::TimeZone;
    use pretty_assertions::assert_eq;

    fn ts() -> DateTime<Utc> {
        Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11)
    }

    fn tags() -> HashMap<String, String> {
        vec![
            ("normal_tag".to_owned(), "value".to_owned()),
            ("true_tag".to_owned(), "true".to_owned()),
            ("empty_tag".to_owned(), "".to_owned()),
        ].into_iter().collect()
    }

    #[test]
    fn test_encode_timestamp() {
        let start = Utc::now().timestamp_nanos();
        assert_eq!(encode_timestamp(Some(ts())), 1542182950000000011);
        assert!(encode_timestamp(None) >= start)
    }

    #[test]
    fn test_encode_namespace() {
        assert_eq!(encode_namespace("services", "status"), "services.status");
        assert_eq!(encode_namespace("", "status"), "status")
    }

    #[test]
    fn test_encode_key() {
        assert_eq!(encode_key("measurement_name".to_string()), "measurement_name");
        assert_eq!(encode_key("measurement name".to_string()), "measurement\\ name");
        assert_eq!(encode_key("measurement=name".to_string()), "measurement\\=name");
        assert_eq!(encode_key("measurement,name".to_string()), "measurement\\,name");
    }

    #[test]
    fn test_encode_tags() {
        assert_eq!(encode_tags(tags()), "normal_tag=value,true_tag=true");

        let tags_to_escape = vec![
            ("tag".to_owned(), "val=ue".to_owned()),
            ("name escape".to_owned(), "true".to_owned()),
            ("value_escape".to_owned(), "value escape".to_owned()),
            ("a_first_place".to_owned(), "10".to_owned()),
        ].into_iter().collect();

        assert_eq!(encode_tags(tags_to_escape), "a_first_place=10,name\\ escape=true,tag=val\\=ue,value_escape=value\\ escape");
    }

    #[test]
    fn test_encode_fields() {
        let fields = vec![
            ("field_string".to_owned(), Field::String("string value".to_owned())),
            ("field_string_escape".to_owned(), Field::String("string\\val\"ue".to_owned())),
            ("field_float".to_owned(), Field::Float(123.45)),
            ("escape key".to_owned(), Field::Float(10.0)),
        ].into_iter().collect();

        assert_eq!(encode_fields(fields), "escape\\ key=10,field_float=123.45,field_string=\"string value\",field_string_escape=\"string\\\\val\\\"ue\"");
    }

    #[test]
    fn encode_counter() {
        let events = vec![
            Metric {
                name: "total".into(),
                timestamp: Some(ts()),
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 1.5 },
            },
            Metric {
                name: "check".into(),
                timestamp: Some(ts()),
                tags: Some(tags()),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 1.0 },
            },
        ];

        let line_protocols = encode_events(events, "ns");
        assert_eq!(
            line_protocols,
            vec!["ns.total,metric_type=counter value=1.5 1542182950000000011", "ns.check,metric_type=counter,normal_tag=value,true_tag=true value=1 1542182950000000011", ]
        );
    }

    #[test]
    fn encode_gauge() {
        let events = vec![Metric {
            name: "meter".to_owned(),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Gauge { value: -1.5 },
        }];

        let line_protocols = encode_events(events, "ns");
        assert_eq!(
            line_protocols,
            vec!["ns.meter,metric_type=gauge,normal_tag=value,true_tag=true value=-1.5 1542182950000000011", ]
        );
    }

    #[test]
    fn encode_set() {
        let events = vec![Metric {
            name: "users".into(),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Set {
                values: vec!["alice".into(), "bob".into()].into_iter().collect(),
            },
        }];

        let line_protocols = encode_events(events, "ns");
        assert_eq!(
            line_protocols,
            vec!["ns.users,metric_type=set,normal_tag=value,true_tag=true value=2 1542182950000000011", ]
        );
    }
}
