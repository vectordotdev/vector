use std::collections::BTreeMap;

#[allow(warnings, clippy::pedantic, clippy::nursery)]
mod ddmetric_proto {
    include!(concat!(env!("OUT_DIR"), "/datadog.agentpayload.rs"));
}

use ddmetric_proto::{
    metric_payload::{MetricSeries, MetricType},
    MetricPayload,
};
use tracing::info;
use vector::common::datadog::DatadogSeriesMetric;

use self::ddmetric_proto::metric_payload::{MetricPoint, Resource};

use super::*;

const SERIES_ENDPOINT_V1: &str = "/api/v1/series";
const SERIES_ENDPOINT_V2: &str = "/api/v2/series";

// unique identification of a Series
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
struct SeriesContext {
    metric_name: String,
    tags: Vec<String>,
    r#type: i32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
struct TimeBucket(i64, i64);

fn get_time_bucket(point: &MetricPoint, interval: i64, metric_type: MetricType) -> TimeBucket {
    match metric_type {
        MetricType::Unspecified => panic!("received an unspecified metric type"),
        MetricType::Rate => TimeBucket(point.timestamp - interval, point.timestamp),
        MetricType::Gauge | MetricType::Count => TimeBucket(point.timestamp, point.timestamp),
    }
}

type TimeSeriesData = BTreeMap<TimeBucket, Vec<f64>>;

/// This type represents the massaged intake data collected from the upstream.
/// The idea is to be able to store what was received in a way that allows us to
/// compare what is important to compare, and accounting for the bits that are not
/// guaranteed to line up.
///
/// For instance, the services that are running, may start at different times, thus the
/// timestamps (TimeBucket) for data points received are not guaranteed to match up.
type SeriesIntake = BTreeMap<SeriesContext, TimeSeriesData>;

// massages the raw payloads into our intake structure
fn generate_series_intake(payloads: &[MetricPayload]) -> SeriesIntake {
    let mut intake = BTreeMap::new();

    payloads.iter().for_each(|payload| {
        payload.series.iter().for_each(|serie| {
            // filter out the metrics we don't care about (ones not generated by the client)
            if !serie.metric.starts_with("foo_metric") {
                return;
            }

            let ctx = SeriesContext {
                metric_name: serie.metric.clone(),
                tags: serie.tags.clone(),
                r#type: serie.r#type,
            };

            if !intake.contains_key(&ctx) {
                intake.insert(ctx.clone(), BTreeMap::new());
            }
            let entry: &mut BTreeMap<TimeBucket, Vec<f64>> = intake.get_mut(&ctx).unwrap();

            serie.points.iter().for_each(|point| {
                let tb = get_time_bucket(point, serie.interval, serie.r#type());
                if !entry.contains_key(&tb) {
                    entry.insert(tb.clone(), vec![]);
                }
                entry.get_mut(&tb).unwrap().push(point.value);
            });
        });
    });

    intake
}

// runs assertions that each set of payloads should be true to regardless
// of the pipeline
fn common_series_assertions(series: &SeriesIntake) {
    // we should have received some metrics from the emitter
    assert!(!series.is_empty());
    info!("metric series received: {}", series.len());

    // specifically we should have received each of these
    let mut found = [
        // NOTE: no count expected due to the in-app type being Rate
        // (https://docs.datadoghq.com/metrics/types/?tab=count#submission-types-and-datadog-in-app-types)
        (false, "rate"),
        (false, "gauge"),
        (false, "set"),
        (false, "histogram"),
    ];
    series.keys().for_each(|ctx| {
        found.iter_mut().for_each(|found| {
            if ctx
                .metric_name
                .starts_with(&format!("foo_metric.{}", found.1))
            {
                info!("received {}", found.1);
                found.0 = true;
            }
        });
    });

    found
        .iter()
        .for_each(|(found, mtype)| assert!(found, "Didn't receive metric type {}", *mtype));
}

impl From<&DatadogSeriesMetric> for MetricSeries {
    fn from(input: &DatadogSeriesMetric) -> Self {
        let mut resources = vec![];
        if let Some(host) = &input.host {
            resources.push(Resource {
                r#type: "host".to_string(),
                name: host.clone(),
            });
        }

        let mut points = vec![];
        input.points.iter().for_each(|point| {
            points.push(MetricPoint {
                value: point.1,
                timestamp: point.0,
            })
        });

        let interval = input.interval.unwrap_or(0) as i64;

        let r#type = match input.r#type {
            vector::common::datadog::DatadogMetricType::Gauge => 3,
            vector::common::datadog::DatadogMetricType::Count => 1,
            vector::common::datadog::DatadogMetricType::Rate => 2,
        };

        MetricSeries {
            resources,
            metric: input.metric.clone(),
            tags: input.tags.clone().unwrap_or_default(),
            points,
            r#type,
            unit: "".to_string(),
            source_type_name: input.clone().source_type_name.unwrap_or_default(),
            interval,
            metadata: None,
        }
    }
}

fn convert_v1_payloads_v2(input: &[DatadogSeriesMetric]) -> Vec<MetricPayload> {
    let mut output = vec![];

    input.iter().for_each(|serie| {
        output.push(MetricPayload {
            series: vec![serie.into()],
        });
    });

    output
}

fn unpack_v1_series(in_payloads: &[FakeIntakePayloadJson]) -> Vec<DatadogSeriesMetric> {
    let mut out_series = vec![];

    in_payloads.iter().for_each(|payload| {
        let series = payload.data.as_array().unwrap();

        series.iter().for_each(|serie| {
            let ser: DatadogSeriesMetric = serde_json::from_value(serie.clone()).unwrap();
            out_series.push(ser);
        });
    });

    out_series
}

async fn get_v1_series_from_pipeline(address: String) -> SeriesIntake {
    info!("getting v1 series payloads");
    let payloads =
        get_fakeintake_payloads::<FakeIntakeResponseJson>(&address, SERIES_ENDPOINT_V1).await;

    info!("unpacking payloads");
    let payloads = unpack_v1_series(&payloads.payloads);
    info!("converting payloads");
    let payloads = convert_v1_payloads_v2(&payloads);

    info!("aggregating payloads");
    let intake = generate_series_intake(&payloads);

    common_series_assertions(&intake);

    info!("{:?}", intake);

    intake
}

async fn get_v2_series_from_pipeline(address: String) -> SeriesIntake {
    info!("getting v2 series payloads");
    let payloads =
        get_fakeintake_payloads::<FakeIntakeResponseRaw>(&address, SERIES_ENDPOINT_V2).await;

    info!("unpacking payloads");
    let payloads = unpack_proto_payloads::<MetricPayload>(&payloads);

    info!("aggregating payloads");
    let intake = generate_series_intake(&payloads);

    common_series_assertions(&intake);

    info!("{:?}", intake);

    intake
}

pub(super) async fn validate() {
    info!("==== getting series data from agent-only pipeline ==== ");
    let agent_intake = get_v2_series_from_pipeline(fake_intake_agent_address()).await;

    info!("==== getting series data from agent-vector pipeline ====");
    let vector_intake = get_v1_series_from_pipeline(fake_intake_vector_address()).await;

    assert_eq!(
        agent_intake.len(),
        vector_intake.len(),
        "different number of unique Series contexts received"
    );

    agent_intake
        .iter()
        .zip(vector_intake.iter())
        .for_each(|(agent_ts, vector_ts)| {
            assert_eq!(agent_ts.0, vector_ts.0, "Mismatch of series context");

            let metric_type = agent_ts.0.r#type;

            // gauge: last one wins.
            // we can't rely on comparing each value due to the fact that we can't guarantee consistent sampling
            if metric_type == 3 {
                let last_agent_point = agent_ts.1.iter().last();
                let last_vector_point = vector_ts.1.iter().last();

                assert_eq!(
                    last_agent_point, last_vector_point,
                    "Mismatch of gauge data"
                );
            }

            // rate: summation.
            if metric_type == 2 {
                let mut agent_sum = 0.0;
                agent_ts
                    .1
                    .iter()
                    .for_each(|(_tb, points)| points.iter().for_each(|v| agent_sum += v));

                let mut vector_sum = 0.0;
                vector_ts
                    .1
                    .iter()
                    .for_each(|(_tb, points)| points.iter().for_each(|v| vector_sum += v));

                assert_eq!(agent_sum, vector_sum, "Mismatch of rate data");
            }
        });
}
