use crate::event::{Event, Metric};
use crate::sinks::util::http::HttpEventEncoder;
use bytes::BytesMut;
use vector_core::event::MetricTags;

pub struct SensissionEncoder {}

impl SensissionEncoder {
    pub const fn new() -> Self {
        Self {}
    }

    fn serialize(event: Event) -> Result<Option<String>, String> {
        let metric: Metric = match event.try_into_metric() {
            None => return Ok(None),
            Some(metric) => metric,
        };

        let ts: String = metric
            .timestamp()
            .map_or(0, |ts| ts.timestamp_micros())
            .to_string();
        let class: String = format!(
            "{}{}",
            metric
                .namespace()
                .map_or("".to_string(), |ns| format!("{}.", ns.to_owned())),
            metric.name()
        );
        let labels: String = Self::format_labels(metric.tags());

        let sensission: String =
            format!("{}// {}{{{}}} {}\n", ts, class, labels, metric.data().value);

        Ok(Some(sensission))
    }

    fn format_labels(tags: Option<&MetricTags>) -> String {
        let str: Vec<String> = tags
            .unwrap_or(&MetricTags::default())
            .iter_single()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        str.join(",")
    }
}

impl Default for SensissionEncoder {
    fn default() -> Self {
        SensissionEncoder::new()
    }
}

impl HttpEventEncoder<BytesMut> for SensissionEncoder {
    fn encode_event(&mut self, event: Event) -> Option<BytesMut> {
        match SensissionEncoder::serialize(event) {
            Ok(str_opt) => str_opt.map(|str| BytesMut::from(str.as_bytes())),
            Err(_err) => None,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::sinks::util::http::HttpEventEncoder;
    use crate::sinks::warp10::encoder::SensissionEncoder;
    use bytes::BytesMut;
    use chrono::{DateTime, NaiveDateTime, Utc};
    use vector_core::event::{Event, Metric, MetricKind, MetricValue};
    use vector_core::metric_tags;

    #[test]
    fn encoding() {
        let mut encoder: SensissionEncoder = SensissionEncoder::new(); // ??? why mut

        let ts: NaiveDateTime = NaiveDateTime::from_timestamp(1669107991, 0);

        let metric: Metric = Metric::new(
            "my.counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 42_f64 },
        )
        .with_namespace(Some("vector"))
        .with_timestamp(Some(DateTime::from_utc(ts, Utc)))
        .with_tags(Some(metric_tags!("host" => "vm1", "region" => "eu")));

        let event: Event = Event::Metric(metric);
        let result: Option<BytesMut> = encoder.encode_event(event);

        let sensission: String =
            "1669107991000000// vector.my.counter{host=vm1,region=eu} 42\n".to_string();
        let expected = Some(BytesMut::from(sensission.as_bytes()));

        assert_eq!(result, expected)
    }
}
