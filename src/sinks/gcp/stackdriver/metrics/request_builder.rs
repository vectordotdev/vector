use std::io;

use bytes::Bytes;
use chrono::Utc;
use vector_lib::event::{Metric, MetricValue};

use crate::sinks::{gcp, prelude::*, util::http::HttpRequest};

#[derive(Clone, Debug)]
pub(super) struct StackdriverMetricsRequestBuilder {
    pub(super) encoder: StackdriverMetricsEncoder,
}

impl RequestBuilder<Vec<Metric>> for StackdriverMetricsRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Metric>;
    type Encoder = StackdriverMetricsEncoder;
    type Payload = Bytes;
    type Request = HttpRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut events: Vec<Metric>,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        (finalizers, builder, events)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        HttpRequest::new(payload.into_payload(), metadata, request_metadata)
    }
}

#[derive(Clone, Debug)]
pub struct StackdriverMetricsEncoder {
    pub(super) default_namespace: String,
    pub(super) started: chrono::DateTime<Utc>,
    pub(super) resource: gcp::GcpTypedResource,
}

impl encoding::Encoder<Vec<Metric>> for StackdriverMetricsEncoder {
    /// Create the object defined [here][api_docs].
    ///
    /// [api_docs]: https://cloud.google.com/monitoring/api/ref_v3/rest/v3/projects.timeSeries/create
    fn encode_input(
        &self,
        input: Vec<Metric>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut byte_size = telemetry().create_request_count_byte_size();
        let time_series = input
            .into_iter()
            .map(|metric| {
                byte_size.add_event(&metric, metric.estimated_json_encoded_size_of());

                let (series, data, _metadata) = metric.into_parts();
                let namespace = series
                    .name
                    .namespace
                    .unwrap_or_else(|| self.default_namespace.clone());
                let metric_type = format!(
                    "custom.googleapis.com/{}/metrics/{}",
                    namespace, series.name.name
                );

                let end_time = data.time.timestamp.unwrap_or_else(chrono::Utc::now);

                let (point_value, interval, metric_kind) = match &data.value {
                    MetricValue::Counter { value } => {
                        let interval = gcp::GcpInterval {
                            start_time: Some(self.started),
                            end_time,
                        };

                        (*value, interval, gcp::GcpMetricKind::Cumulative)
                    }
                    MetricValue::Gauge { value } => {
                        let interval = gcp::GcpInterval {
                            start_time: None,
                            end_time,
                        };

                        (*value, interval, gcp::GcpMetricKind::Gauge)
                    }
                    _ => {
                        unreachable!("sink has filtered out all metrics that aren't counter or gauge by this point")
                    },
                };
                let metric_labels = series
                    .tags
                    .unwrap_or_default()
                    .into_iter_single()
                    .collect::<std::collections::HashMap<_, _>>();

                gcp::GcpSerie {
                    metric: gcp::GcpMetric {
                        r#type: metric_type,
                        labels: metric_labels,
                    },
                    resource: gcp::GcpResource {
                        r#type: self.resource.r#type.clone(),
                        labels: self.resource.labels.clone(),
                    },
                    metric_kind,
                    value_type: gcp::GcpValueType::Int64,
                    points: vec![gcp::GcpPoint {
                        interval,
                        value: gcp::GcpPointValue {
                            int64_value: Some(point_value as i64),
                        },
                    }],
                }
            })
            .collect::<Vec<_>>();

        let series = gcp::GcpSeries {
            time_series: &time_series,
        };

        let body = crate::serde::json::to_bytes(&series).unwrap().freeze();
        writer.write_all(&body).map(|()| (body.len(), byte_size))
    }
}
