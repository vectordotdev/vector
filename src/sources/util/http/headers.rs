use bytes::Bytes;
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    event::Event,
    lookup::path,
};
use warp::http::{HeaderMap, HeaderValue};

use crate::{event::Value, sources::http_server::HttpConfigParamKind};

pub fn add_headers(
    events: &mut [Event],
    headers_config: &[HttpConfigParamKind],
    headers: &HeaderMap,
    log_namespace: LogNamespace,
    source_name: &'static str,
) {
    for h in headers_config {
        match h {
            // Add each non-wildcard containing header that was specified
            // in the `headers` config option to the event if an exact match
            // is found.
            HttpConfigParamKind::Exact(header_name) => {
                let value = headers.get(header_name).map(HeaderValue::as_bytes);

                for event in events.iter_mut() {
                    match event {
                        Event::Log(log) => {
                            log_namespace.insert_source_metadata(
                                source_name,
                                log,
                                Some(LegacyKey::InsertIfEmpty(path!(header_name))),
                                path!("headers", header_name),
                                Value::from(value.map(Bytes::copy_from_slice)),
                            );
                        }
                        Event::Metric(_) | Event::Trace(_) => {
                            event.metadata_mut().value_mut().insert(
                                path!(source_name, "headers", header_name),
                                Value::from(value.map(Bytes::copy_from_slice)),
                            );
                        }
                    }
                }
            }
            // Add all headers that match against wildcard pattens specified
            // in the `headers` config option to the event.
            HttpConfigParamKind::Glob(header_pattern) => {
                for header_name in headers.keys() {
                    if header_pattern
                        .matches_with(header_name.as_str(), glob::MatchOptions::default())
                    {
                        let value = headers.get(header_name).map(HeaderValue::as_bytes);

                        for event in events.iter_mut() {
                            match event {
                                Event::Log(log) => {
                                    log_namespace.insert_source_metadata(
                                        source_name,
                                        log,
                                        Some(LegacyKey::InsertIfEmpty(path!(header_name.as_str()))),
                                        path!("headers", header_name.as_str()),
                                        Value::from(value.map(Bytes::copy_from_slice)),
                                    );
                                }
                                Event::Metric(_) | Event::Trace(_) => {
                                    event.metadata_mut().value_mut().insert(
                                        path!(source_name, "headers", header_name.as_str()),
                                        Value::from(value.map(Bytes::copy_from_slice)),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use std::time::SystemTime;
    use vector_lib::config::LogNamespace;
    use vrl::{path, value};
    use warp::http::HeaderMap;

    use crate::event::{Event, MetricKind, MetricTags, MetricValue};
    use crate::{
        event::{LogEvent, Metric, TraceEvent},
        sources::{http_server::HttpConfigParamKind, util::add_headers},
    };

    #[test]
    fn multiple_headers() {
        let header_names = [
            HttpConfigParamKind::Exact("Content-Type".into()),
            HttpConfigParamKind::Exact("User-Agent".into()),
        ];
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", "application/x-protobuf".parse().unwrap());
        headers.insert("User-Agent", "Test".parse().unwrap());
        headers.insert("Content-Encoding", "gzip".parse().unwrap());

        let mut base_log = [LogEvent::from(value!({})).into()];
        add_headers(
            &mut base_log,
            &header_names,
            &headers,
            LogNamespace::Legacy,
            "test",
        );
        let mut namespaced_log = [LogEvent::from(value!({})).into()];
        add_headers(
            &mut namespaced_log,
            &header_names,
            &headers,
            LogNamespace::Vector,
            "test",
        );

        assert_eq!(
            base_log[0].as_log().value(),
            namespaced_log[0]
                .metadata()
                .value()
                .get(path!("test", "headers"))
                .unwrap()
        );

        let mut metric = [Event::from(
            Metric::new(
                "some.random.metric",
                MetricKind::Incremental,
                MetricValue::Counter { value: 123.4 },
            )
            .with_timestamp(Some(DateTime::<Utc>::from(SystemTime::now())))
            .with_tags(Some(MetricTags::default())),
        )];

        add_headers(
            &mut metric,
            &header_names,
            &headers,
            LogNamespace::default(),
            "test",
        );

        let mut trace = [TraceEvent::from(btreemap! {
            "span_id" => "abc123",
            "trace_id" => "xyz789",
            "span_name" => "test-span",
            "service" => "my-service",
        })
        .into()];

        add_headers(
            &mut trace,
            &header_names,
            &headers,
            LogNamespace::default(),
            "test",
        );

        assert_eq!(
            metric[0]
                .metadata()
                .value()
                .get(path!("test", "headers"))
                .unwrap(),
            trace[0]
                .metadata()
                .value()
                .get(path!("test", "headers"))
                .unwrap()
        );
    }

    #[test]
    fn multiple_headers_wildcard() {
        let header_names = [HttpConfigParamKind::Glob(
            glob::Pattern::new("Content-*").unwrap(),
        )];
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", "application/x-protobuf".parse().unwrap());
        headers.insert("User-Agent", "Test".parse().unwrap());
        headers.insert("Content-Encoding", "gzip".parse().unwrap());

        let mut base_log = [LogEvent::from(value!({})).into()];
        add_headers(
            &mut base_log,
            &header_names,
            &headers,
            LogNamespace::Legacy,
            "test",
        );
        let mut namespaced_log = [LogEvent::from(value!({})).into()];
        add_headers(
            &mut namespaced_log,
            &header_names,
            &headers,
            LogNamespace::Vector,
            "test",
        );

        let log = base_log[0].as_log();
        assert_eq!(
            log.value(),
            namespaced_log[0]
                .metadata()
                .value()
                .get(path!("test", "headers"))
                .unwrap(),
            "Checking legacy and namespaced log contain headers string"
        );
        assert_eq!(
            log["content-type"],
            "application/x-protobuf".into(),
            "Checking log contains Content-Type header"
        );
        assert!(
            !log.contains("user-agent"),
            "Checking log does not contain User-Agent header"
        );
        assert_eq!(
            log["content-encoding"],
            "gzip".into(),
            "Checking log contains Content-Encoding header"
        );

        let mut metric = [Event::from(
            Metric::new(
                "some.random.metric",
                MetricKind::Incremental,
                MetricValue::Counter { value: 123.4 },
            )
            .with_timestamp(Some(DateTime::<Utc>::from(SystemTime::now())))
            .with_tags(Some(MetricTags::default())),
        )];

        add_headers(
            &mut metric,
            &header_names,
            &headers,
            LogNamespace::default(),
            "test",
        );

        let metric_headers = metric[0]
            .metadata()
            .value()
            .get(path!("test", "headers"))
            .unwrap();

        assert_eq!(
            metric_headers.get("content-type").unwrap(),
            &value!("application/x-protobuf"),
            "Checking metric contains Content-Type header"
        );
        assert!(
            !metric_headers.contains("user-agent"),
            "Checking metric does not contain User-Agent header"
        );
        assert_eq!(
            metric_headers.get("content-encoding").unwrap(),
            &value!("gzip"),
            "Checking metric contains Content-Encoding header"
        );

        let mut trace = [TraceEvent::from(btreemap! {
            "span_id" => "abc123",
            "trace_id" => "xyz789",
            "span_name" => "test-span",
            "service" => "my-service",
        })
        .into()];

        add_headers(
            &mut trace,
            &header_names,
            &headers,
            LogNamespace::default(),
            "test",
        );

        let trace_headers = trace[0]
            .metadata()
            .value()
            .get(path!("test", "headers"))
            .unwrap();

        assert_eq!(
            trace_headers.get("content-type").unwrap(),
            &value!("application/x-protobuf"),
            "Checking trace contains Content-Type header"
        );
        assert!(
            !trace_headers.contains("user-agent"),
            "Checking trace does not contain User-Agent header"
        );
        assert_eq!(
            trace_headers.get("content-encoding").unwrap(),
            &value!("gzip"),
            "Checking trace contains Content-Encoding header"
        );
    }
}
