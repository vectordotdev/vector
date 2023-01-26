use async_trait::async_trait;
use codecs::JsonSerializerConfig;
use futures::StreamExt;
use futures_util::stream::BoxStream;
use indoc::indoc;
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;
use vector_core::{sink::StreamSink, transform::Transform};

use super::{host_key, logs::HumioLogsConfig};
use crate::{
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, TransformConfig,
        TransformContext,
    },
    event::{Event, EventArray, EventContainer},
    sinks::{
        splunk_hec::common::SplunkHecDefaultBatchSettings,
        util::{BatchConfig, Compression, TowerRequestConfig},
        Healthcheck, VectorSink,
    },
    template::Template,
    tls::TlsConfig,
    transforms::{metric_to_log::MetricToLogConfig, OutputBuffer},
};

/// Configuration for the `humio_metrics` sink.
//
// TODO: This sink overlaps almost entirely with the `humio_logs` sink except for the metric-to-log
// transform that it uses to get metrics into the shape of a log before sending to Humio. However,
// due to issues with aliased fields and flattened fields [1] in `serde`, we can't embed the
// `humio_logs` config here.
//
// [1]: https://github.com/serde-rs/serde/issues/1504
#[configurable_component(sink("humio_metrics"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct HumioMetricsConfig {
    #[serde(flatten)]
    transform: MetricToLogConfig,

    /// The Humio ingestion token.
    token: SensitiveString,

    /// The base URL of the Humio instance.
    #[serde(alias = "host")]
    pub(super) endpoint: Option<String>,

    /// The source of events sent to this sink.
    ///
    /// Typically the filename the metrics originated from. Maps to `@source` in Humio.
    source: Option<Template>,

    /// The type of events sent to this sink. Humio uses this as the name of the parser to use to ingest the data.
    ///
    /// If unset, Humio will default it to none.
    event_type: Option<Template>,

    /// Overrides the name of the log field used to grab the hostname to send to Humio.
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    #[serde(default = "host_key")]
    host_key: String,

    /// Event fields to be added to Humio’s extra fields.
    ///
    /// Can be used to tag events by specifying fields starting with `#`.
    ///
    /// For more information, see [Humio’s Format of Data][humio_data_format].
    ///
    /// [humio_data_format]: https://docs.humio.com/integrations/data-shippers/hec/#format-of-data
    #[serde(default)]
    indexed_fields: Vec<String>,

    /// Optional name of the repository to ingest into.
    ///
    /// In public-facing APIs, this must (if present) be equal to the repository used to create the ingest token used for authentication.
    ///
    /// In private cluster setups, Humio can be configured to allow these to be different.
    ///
    /// For more information, see [Humio’s Format of Data][humio_data_format].
    ///
    /// [humio_data_format]: https://docs.humio.com/integrations/data-shippers/hec/#format-of-data
    #[serde(default)]
    index: Option<Template>,

    #[configurable(derived)]
    #[serde(default)]
    compression: Compression,

    #[configurable(derived)]
    #[serde(default)]
    request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(default)]
    batch: BatchConfig<SplunkHecDefaultBatchSettings>,

    #[configurable(derived)]
    tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for HumioMetricsConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
                host_key = "hostname"
                token = "${HUMIO_TOKEN}"
            "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for HumioMetricsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let transform = self
            .transform
            .clone()
            .build(&TransformContext::new_with_globals(cx.globals.clone()))
            .await?;

        let sink = HumioLogsConfig {
            token: self.token.clone(),
            endpoint: self.endpoint.clone(),
            source: self.source.clone(),
            encoding: JsonSerializerConfig::default().into(),
            event_type: self.event_type.clone(),
            host_key: self.host_key.clone(),
            indexed_fields: self.indexed_fields.clone(),
            index: self.index.clone(),
            compression: self.compression,
            request: self.request,
            batch: self.batch,
            tls: self.tls.clone(),
            timestamp_nanos_key: None,
            acknowledgements: Default::default(),
            // hard coded as humio expects this format so no sense in making it configurable
            timestamp_key: "timestamp".to_string(),
        };

        let (sink, healthcheck) = sink.clone().build(cx).await?;

        let sink = HumioMetricsSink {
            inner: sink,
            transform,
        };

        Ok((VectorSink::Stream(Box::new(sink)), healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

pub struct HumioMetricsSink {
    inner: VectorSink,
    transform: Transform,
}

#[async_trait]
impl StreamSink<EventArray> for HumioMetricsSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, EventArray>) -> Result<(), ()> {
        let mut transform = self.transform;
        self.inner
            .run(input.map(move |events| {
                let mut buf = OutputBuffer::with_capacity(events.len());
                for event in events.into_events() {
                    transform.as_function().transform(&mut buf, event);
                }
                // Awkward but necessary for the `EventArray` type
                let events = buf.into_events().map(Event::into_log).collect::<Vec<_>>();
                events.into()
            }))
            .await
    }
}

#[cfg(test)]
mod tests {
    use chrono::{offset::TimeZone, Utc};
    use futures::stream;
    use indoc::indoc;
    use similar_asserts::assert_eq;
    use vector_core::metric_tags;

    use super::*;
    use crate::{
        event::{
            metric::{MetricKind, MetricValue, StatisticKind},
            Event, Metric,
        },
        sinks::util::test::{build_test_server, load_sink},
        test_util::{
            self,
            components::{run_and_assert_sink_compliance, HTTP_SINK_TAGS},
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<HumioMetricsConfig>();
    }

    #[test]
    fn test_endpoint_field() {
        let (config, _) = load_sink::<HumioMetricsConfig>(indoc! {r#"
            token = "atoken"
            batch.max_events = 1
            endpoint = "https://localhost:9200/"
        "#})
        .unwrap();

        assert_eq!(Some("https://localhost:9200/".to_string()), config.endpoint);
        let (config, _) = load_sink::<HumioMetricsConfig>(indoc! {r#"
            token = "atoken"
            batch.max_events = 1
            host = "https://localhost:9200/"
        "#})
        .unwrap();

        assert_eq!(Some("https://localhost:9200/".to_string()), config.endpoint);
    }

    #[tokio::test]
    async fn smoke_json() {
        let (mut config, cx) = load_sink::<HumioMetricsConfig>(indoc! {r#"
            token = "atoken"
            batch.max_events = 1
        "#})
        .unwrap();

        let addr = test_util::next_addr();
        // Swap out the endpoint so we can force send it
        // to our local server
        let endpoint = format!("http://{}", addr);
        config.endpoint = Some(endpoint.clone());

        let (sink, _) = config.build(cx).await.unwrap();

        let (rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

        // Make our test metrics.
        let metrics = vec![
            Event::from(
                Metric::new(
                    "metric1",
                    MetricKind::Incremental,
                    MetricValue::Counter { value: 42.0 },
                )
                .with_tags(Some(metric_tags!("os.host" => "somehost")))
                .with_timestamp(Some(
                    Utc.ymd(2020, 8, 18)
                        .and_hms_opt(21, 0, 1)
                        .expect("invalid timestamp"),
                )),
            ),
            Event::from(
                Metric::new(
                    "metric2",
                    MetricKind::Absolute,
                    MetricValue::Distribution {
                        samples: vector_core::samples![1.0 => 100, 2.0 => 200, 3.0 => 300],
                        statistic: StatisticKind::Histogram,
                    },
                )
                .with_tags(Some(metric_tags!("os.host" => "somehost")))
                .with_timestamp(Some(
                    Utc.ymd(2020, 8, 18)
                        .and_hms_opt(21, 0, 2)
                        .expect("invalid timestamp"),
                )),
            ),
        ];

        let len = metrics.len();
        run_and_assert_sink_compliance(sink, stream::iter(metrics), &HTTP_SINK_TAGS).await;

        let output = rx.take(len).collect::<Vec<_>>().await;
        assert_eq!(
            r#"{"event":{"counter":{"value":42.0},"kind":"incremental","name":"metric1","tags":{"os.host":"somehost"}},"fields":{},"time":1597784401.0}"#,
            output[0].1
        );
        assert_eq!(
            r#"{"event":{"distribution":{"samples":[{"rate":100,"value":1.0},{"rate":200,"value":2.0},{"rate":300,"value":3.0}],"statistic":"histogram"},"kind":"absolute","name":"metric2","tags":{"os.host":"somehost"}},"fields":{},"time":1597784402.0}"#,
            output[1].1
        );
    }

    #[tokio::test]
    async fn multi_value_tags() {
        let (mut config, cx) = load_sink::<HumioMetricsConfig>(indoc! {r#"
            token = "atoken"
            batch.max_events = 1
            metric_tag_values = "full"
        "#})
        .unwrap();

        let addr = test_util::next_addr();
        // Swap out the endpoint so we can force send it
        // to our local server
        let endpoint = format!("http://{}", addr);
        config.endpoint = Some(endpoint.clone());

        let (sink, _) = config.build(cx).await.unwrap();

        let (rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

        // Make our test metrics.
        let metrics = vec![Event::from(
            Metric::new(
                "metric1",
                MetricKind::Incremental,
                MetricValue::Counter { value: 42.0 },
            )
            .with_tags(Some(metric_tags!(
                "code" => "200",
                "code" => "success"
            )))
            .with_timestamp(Some(
                Utc.ymd(2020, 8, 18)
                    .and_hms_opt(21, 0, 1)
                    .expect("invalid timestamp"),
            )),
        )];

        let len = metrics.len();
        run_and_assert_sink_compliance(sink, stream::iter(metrics), &HTTP_SINK_TAGS).await;

        let output = rx.take(len).collect::<Vec<_>>().await;
        assert_eq!(
            r#"{"event":{"counter":{"value":42.0},"kind":"incremental","name":"metric1","tags":{"code":["200","success"]}},"fields":{},"time":1597784401.0}"#,
            output[0].1
        );
    }
}
