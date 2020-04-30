use crate::{
    dns::Resolver,
    event::{
        metric::{Metric, MetricKind, MetricValue},
        Event,
    },
    sinks::util::{
        http::{BatchedHttpSink, HttpClient, HttpSink},
        BatchEventsConfig, MetricBuffer, TowerRequestConfig,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use chrono::{DateTime, Utc};
use futures01::{Future, Sink};
use http::{uri::InvalidUri, StatusCode, Uri};
use hyper;
use lazy_static::lazy_static;
use prost::Message;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::BTreeMap;
use std::sync::atomic::{
    AtomicI64,
    Ordering::{Acquire, Release},
};
use tower::Service;

pub mod signalfx_proto {
    // pub mod signalx_proto {
    //     include!(concat!(
    //         env!("OUT_DIR"),
    //         "/com.signalfx.metrics.protobuf.rs"
    //     ));

    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Datum {
        #[prost(string, optional, tag = "1")]
        pub str_value: ::std::option::Option<std::string::String>,
        #[prost(double, optional, tag = "2")]
        pub double_value: ::std::option::Option<f64>,
        #[prost(int64, optional, tag = "3")]
        pub int_value: ::std::option::Option<i64>,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Dimension {
        #[prost(string, optional, tag = "1")]
        pub key: ::std::option::Option<std::string::String>,
        #[prost(string, optional, tag = "2")]
        pub value: ::std::option::Option<std::string::String>,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct DataPoint {
        #[prost(string, optional, tag = "1")]
        pub source: ::std::option::Option<std::string::String>,
        #[prost(string, optional, tag = "2")]
        pub metric: ::std::option::Option<std::string::String>,
        #[prost(int64, optional, tag = "3")]
        pub timestamp: ::std::option::Option<i64>,
        #[prost(message, optional, tag = "4")]
        pub value: ::std::option::Option<Datum>,
        #[prost(enumeration = "MetricType", optional, tag = "5")]
        pub metric_type: ::std::option::Option<i32>,
        #[prost(message, repeated, tag = "6")]
        pub dimensions: ::std::vec::Vec<Dimension>,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct DataPointUploadMessage {
        #[prost(message, repeated, tag = "1")]
        pub datapoints: ::std::vec::Vec<DataPoint>,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct PointValue {
        #[prost(int64, optional, tag = "3")]
        pub timestamp: ::std::option::Option<i64>,
        #[prost(message, optional, tag = "4")]
        pub value: ::std::option::Option<Datum>,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Property {
        #[prost(string, optional, tag = "1")]
        pub key: ::std::option::Option<std::string::String>,
        #[prost(message, optional, tag = "2")]
        pub value: ::std::option::Option<PropertyValue>,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct PropertyValue {
        #[prost(string, optional, tag = "1")]
        pub str_value: ::std::option::Option<std::string::String>,
        #[prost(double, optional, tag = "2")]
        pub double_value: ::std::option::Option<f64>,
        #[prost(int64, optional, tag = "3")]
        pub int_value: ::std::option::Option<i64>,
        #[prost(bool, optional, tag = "4")]
        pub bool_value: ::std::option::Option<bool>,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Event {
        #[prost(string, required, tag = "1")]
        pub event_type: std::string::String,
        #[prost(message, repeated, tag = "2")]
        pub dimensions: ::std::vec::Vec<Dimension>,
        #[prost(message, repeated, tag = "3")]
        pub properties: ::std::vec::Vec<Property>,
        #[prost(enumeration = "EventCategory", optional, tag = "4")]
        pub category: ::std::option::Option<i32>,
        #[prost(int64, optional, tag = "5")]
        pub timestamp: ::std::option::Option<i64>,
    }
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct EventUploadMessage {
        #[prost(message, repeated, tag = "1")]
        pub events: ::std::vec::Vec<Event>,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum MetricType {
        ///*
        /// Numerical: Periodic, instantaneous measurement of some state.
        Gauge = 0,
        ///*
        /// Numerical: Count of occurrences. Generally non-negative integers.
        Counter = 1,
        ///*
        /// String: Used for non-continuous quantities (that is, measurements where there is a fixed
        /// set of meaningful values). This is essentially a special case of gauge.
        Enum = 2,
        ///*
        /// Tracks a value that increases over time, where only the difference is important.
        CumulativeCounter = 3,
    }
    ///*
    /// Different categories of events supported
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum EventCategory {
        ///*
        /// Created by user via UI or API, e.g. a deployment event
        UserDefined = 1000000,
        ///*
        /// Output by anomaly detectors
        Alert = 100000,
        ///*
        /// Audit trail events
        Audit = 200000,
        ///*
        /// Generated by analytics server
        Job = 300000,
        ///*
        /// @deprecated
        /// Event originated within collectd (deprecated in favor of AGENT)
        Collectd = 400000,
        ///*
        /// Service discovery event
        ServiceDiscovery = 500000,
        ///*
        /// Created by exception appenders to denote exceptional events
        Exception = 700000,
        ///*
        /// Event originated from an agent
        Agent = 2000000,
    }
    //}
}

pub fn default_realm() -> String {
    String::from("us0")
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct SignalFxConfig {
    // TODO: namespace a du sens dans le contexte SignalFx ? il préfixe les métriques
    pub namespace: String,
    #[serde(default = "default_realm")]
    pub realm: String,
    pub token: String,
    #[serde(default)]
    pub batch: BatchEventsConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
}

struct SignalFxSink {
    config: SignalFxConfig,
    last_sent_timestamp: AtomicI64,
    uri: Uri,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        retry_attempts: Some(5),
        ..Default::default()
    };
}

inventory::submit! {
    SinkDescription::new::<SignalFxConfig>("signalfx_metrics")
}

#[typetag::serde(name = "signalfx_metrics")]
impl SinkConfig for SignalFxConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let healthcheck = healthcheck(self.clone(), cx.resolver())?;

        let batch = self.batch.unwrap_or(20, 1);
        let request = self.request.unwrap_with(&REQUEST_DEFAULTS);

        let uri = format!("https://ingest.{}.signalfx.com/v2/datapoint", self.realm)
            .parse::<Uri>()
            .context(super::UriParseError)?;
        let timestamp = Utc::now().timestamp();

        let sink = SignalFxSink {
            config: self.clone(),
            uri,
            last_sent_timestamp: AtomicI64::new(timestamp),
        };

        let sink = BatchedHttpSink::new(sink, MetricBuffer::new(), request, batch, None, &cx)
            .sink_map_err(|e| error!("Fatal SignalFx error: {}", e));

        Ok((Box::new(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "signalfx_metrics"
    }
}

impl HttpSink for SignalFxSink {
    type Input = Event;
    type Output = Vec<Metric>;

    fn encode_event(&self, event: Event) -> Option<Self::Input> {
        Some(event)
    }

    fn build_request(&self, events: Self::Output) -> http::Request<Vec<u8>> {
        let now = Utc::now().timestamp();
        let interval = now - self.last_sent_timestamp.load(Acquire);
        self.last_sent_timestamp.store(now, Release);

        let input = encode_events(events, interval, &self.config.namespace);
        let body = serialize_datapoints(&input);

        http::Request::post(self.uri.clone())
            .header("Content-Type", "application/x-protobuf")
            .header("X-SF-Token", self.config.token.clone())
            .body(body)
            .unwrap()
    }
}

fn healthcheck(config: SignalFxConfig, resolver: Resolver) -> crate::Result<super::Healthcheck> {
    // TODO: Need to check if an API endpoint exist to check if the configuration is valid (Realm, Token and network connectivity)
    let uri = format!("https://ingest.{}.signalfx.com/v2/datapoint", config.realm)
        .parse::<Uri>()
        .context(super::UriParseError)?;

    let request = http::Request::get(uri)
        .header("X-SF-Token", config.token)
        .body(hyper::Body::empty())
        .unwrap();

    let mut client = HttpClient::new(resolver, None)?;

    let healthcheck = client
        .call(request)
        .map_err(|err| err.into())
        .and_then(|response| match response.status() {
            StatusCode::OK => Ok(()),
            other => Err(super::HealthcheckError::UnexpectedStatus { status: other }.into()),
        });

    Ok(Box::new(healthcheck))
}

fn encode_dimensions(tags: BTreeMap<String, String>) -> Vec<signalfx_proto::Dimension> {
    let pairs: Vec<signalfx_proto::Dimension> = tags
        .iter()
        .map(|(name, value)| signalfx_proto::Dimension {
            key: Some(name.to_string()),
            value: Some(value.to_string()),
        })
        .collect();
    pairs
}

fn encode_timestamp(timestamp: Option<DateTime<Utc>>) -> i64 {
    match timestamp {
        Some(ts) => ts.timestamp_millis(),
        None => Utc::now().timestamp_millis(),
    }
}

fn serialize_datapoints(dpu: &signalfx_proto::DataPointUploadMessage) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.reserve(dpu.encoded_len());
    dpu.encode(&mut buf).unwrap();
    buf
}

fn encode_namespace(namespace: &str, name: &str) -> String {
    if !namespace.is_empty() {
        format!("{}.{}", namespace, name)
    } else {
        name.to_string()
    }
}

fn encode_events(
    events: Vec<Metric>,
    interval: i64,
    namespace: &str,
) -> signalfx_proto::DataPointUploadMessage {
    let series = events
        .into_iter()
        .filter_map(|event| {
            let fullname = encode_namespace(namespace, &event.name);
            let ts = encode_timestamp(event.timestamp);
            let dimensions = match event.tags.clone().map(encode_dimensions) {
                Some(v) => v,
                None => vec![],
            };

            match event.kind {
                MetricKind::Incremental => match event.value {
                    MetricValue::Counter { value } => Some(vec![signalfx_proto::DataPoint {
                        source: Some(String::from("default")),
                        metric: Some(fullname),
                        metric_type: Some(signalfx_proto::MetricType::Counter as i32),
                        timestamp: Some(ts),
                        value: Some(signalfx_proto::Datum {
                            double_value: Some(value),
                            str_value: None,
                            int_value: None,
                        }),
                        dimensions: dimensions,
                    }]),
                    _ => None,
                },
                MetricKind::Absolute => match event.value {
                    MetricValue::Gauge { value } => Some(vec![signalfx_proto::DataPoint {
                        source: Some(String::from("default")),
                        metric: Some(fullname),
                        metric_type: Some(signalfx_proto::MetricType::Gauge as i32),
                        timestamp: Some(ts),
                        value: Some(signalfx_proto::Datum {
                            double_value: Some(value),
                            str_value: None,
                            int_value: None,
                        }),
                        dimensions: dimensions,
                    }]),
                    _ => None,
                },
            }
        })
        .flatten()
        .collect::<Vec<_>>();

    signalfx_proto::DataPointUploadMessage { datapoints: series }
}
