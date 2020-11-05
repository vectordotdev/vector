use super::logs::HumioLogsConfig;
use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription, TransformConfig},
    sinks::{Healthcheck, VectorSink},
    transforms::metric_to_log::MetricToLogConfig,
};
use futures01::Sink;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HumioMetricsConfig {
    #[serde(flatten)]
    transform: MetricToLogConfig,

    #[serde(flatten)]
    sink: HumioLogsConfig,
}

inventory::submit! {
    SinkDescription::new::<HumioMetricsConfig>("humio_metrics")
}

impl GenerateConfig for HumioMetricsConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"host_key = "hostname"
            token = "${HUMIO_TOKEN}"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "humio_metrics")]
impl SinkConfig for HumioMetricsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let mut transform = self.transform.clone().build().await?;
        let (sink, healthcheck) = self.sink.clone().build(cx).await?;

        let sink = Box::new(sink.into_futures01sink().with_flat_map(move |e| {
            let mut buf = Vec::with_capacity(1);
            transform.as_function().transform(&mut buf, e);
            futures01::stream::iter_ok(buf.into_iter())
        }));

        Ok((VectorSink::Futures01Sink(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "humio_metrics"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{
        metric::{MetricKind, MetricValue, StatisticKind},
        Event, Metric,
    };
    use crate::sinks::util::test::{build_test_server, load_sink};
    use crate::test_util;
    use chrono::{offset::TimeZone, Utc};
    use futures::{stream, StreamExt};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<HumioMetricsConfig>();
    }

    #[tokio::test]
    async fn smoke_json() {
        let (mut config, cx) = load_sink::<HumioMetricsConfig>(
            r#"
            token = "atoken"
            batch.max_events = 1
            "#,
        )
        .unwrap();

        let addr = test_util::next_addr();
        // Swap out the endpoint so we can force send it
        // to our local server
        let endpoint = format!("http://{}", addr);
        config.sink.endpoint = Some(endpoint.clone());

        let (sink, _) = config.build(cx).await.unwrap();

        let (rx, _trigger, server) = build_test_server(addr);
        tokio::spawn(server);

        // Make our test metrics.
        let metrics = vec![
            Event::from(Metric {
                name: "metric1".to_string(),
                namespace: None,
                timestamp: Some(Utc.ymd(2020, 8, 18).and_hms(21, 0, 1)),
                tags: Some(
                    vec![("os.host".to_string(), "somehost".to_string())]
                        .into_iter()
                        .collect(),
                ),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 42.0 },
            }),
            Event::from(Metric {
                name: "metric2".to_string(),
                namespace: None,
                timestamp: Some(Utc.ymd(2020, 8, 18).and_hms(21, 0, 2)),
                tags: Some(
                    vec![("os.host".to_string(), "somehost".to_string())]
                        .into_iter()
                        .collect(),
                ),
                kind: MetricKind::Absolute,
                value: MetricValue::Distribution {
                    values: vec![1.0, 2.0, 3.0],
                    sample_rates: vec![100, 200, 300],
                    statistic: StatisticKind::Histogram,
                },
            }),
        ];

        let len = metrics.len();
        let _ = sink.run(stream::iter(metrics)).await.unwrap();

        let output = rx.take(len).collect::<Vec<_>>().await;
        assert_eq!("{\"event\":{\"counter\":{\"value\":42.0},\"kind\":\"incremental\",\"name\":\"metric1\",\"tags\":{\"os.host\":\"somehost\"}},\"fields\":{},\"time\":1597784401.0}", output[0].1);
        assert_eq!(
            "{\"event\":{\"distribution\":{\"sample_rates\":[100,200,300],\"statistic\":\"histogram\",\"values\":[1.0,2.0,3.0]},\"kind\":\"absolute\",\"name\":\"metric2\",\"tags\":{\"os.host\":\"somehost\"}},\"fields\":{},\"time\":1597784402.0}", output[1].1);
    }
}
