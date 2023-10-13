use std::collections::BTreeMap;

use base64::{prelude::BASE64_STANDARD, Engine};
use bytes::Bytes;
use flate2::read::ZlibDecoder;
use prost::Message;

use vector::test_util::trace_init;

#[allow(warnings, clippy::pedantic, clippy::nursery)]
mod ddmetric_proto {
    include!(concat!(env!("OUT_DIR"), "/datadog.agentpayload.rs"));
}

use super::*;

const SERIES_ENDPOINT: &str = "/api/v2/series";

fn decompress_payload(payload: Vec<u8>) -> std::io::Result<Vec<u8>> {
    let mut decompressor = ZlibDecoder::new(&payload[..]);
    let mut decompressed = Vec::new();
    let result = std::io::copy(&mut decompressor, &mut decompressed);
    result.map(|_| decompressed)
}

fn unpack_payloads_series_v2(
    in_payloads: &FakeIntakeResponseRaw,
) -> Vec<ddmetric_proto::MetricPayload> {
    let mut out_payloads = vec![];

    in_payloads.payloads.iter().for_each(|payload| {
        // decode base64
        let payload = BASE64_STANDARD
            .decode(&payload.data)
            .expect("Invalid base64 data");

        // decompress
        let bytes = Bytes::from(decompress_payload(payload).unwrap());

        let payload = ddmetric_proto::MetricPayload::decode(bytes).unwrap();

        out_payloads.push(payload);
    });

    out_payloads
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
struct Context {
    metric_name: String,
    tags: Vec<String>,
    r#type: i32,
}

// Sums up the metrics in each series by name.
fn aggregate_normalize_series_metrics(
    payloads: &Vec<ddmetric_proto::MetricPayload>,
    // ) -> Vec<ddmetric_proto::metric_payload::MetricSeries> {
) -> BTreeMap<Context, ddmetric_proto::metric_payload::MetricSeries> {
    let mut aggregate = BTreeMap::new();

    for metric_payload in payloads {
        for serie in &metric_payload.series {
            // filter out the metrics we don't care about
            if !serie.metric.starts_with("foo_metric") {
                continue;
            }

            let ctx = Context {
                metric_name: serie.metric.clone(),
                tags: serie.tags.clone(),
                r#type: serie.r#type,
            };

            if !aggregate.contains_key(&ctx) {
                aggregate.insert(ctx, serie.clone());
                continue;
            }

            let existing = aggregate.get_mut(&ctx).unwrap();

            existing.points.extend_from_slice(&serie.points);
        }
    }

    // remove the timestamps and sum the points and normalize the other metadata
    for (_ctx, series) in &mut aggregate {
        let mut value = 0.0;
        for point in &mut series.points {
            point.timestamp = 0;
            value += point.value;
        }
        series.points[0].value = value;

        for resource in &mut series.resources {
            if resource.r#type == "host" {
                if resource.name.ends_with("-vector") {
                    resource
                        .name
                        .truncate(resource.name.len() - "-vector".len());
                }
            }
        }

        if series.r#type() != ddmetric_proto::metric_payload::MetricType::Rate
            && series.interval != 0
        {
            println!(
                "serie {:?} non-rate metric has interval set. Setting to zero.",
                _ctx
            );
            series.interval = 0;
        }
        // println!("{:?} {:?}", _ctx, series);
    }

    aggregate
}

// runs assertions that each set of payloads should be true to regardless
// of the pipeline
fn common_assertions(series: &BTreeMap<Context, ddmetric_proto::metric_payload::MetricSeries>) {
    assert!(series.len() > 0);
    println!("metric series received: {}", series.len());
}

async fn get_series_from_pipeline(
    address: String,
) -> BTreeMap<Context, ddmetric_proto::metric_payload::MetricSeries> {
    println!("getting payloads");
    let payloads =
        get_fakeintake_payloads::<FakeIntakeResponseRaw>(&address, SERIES_ENDPOINT).await;

    println!("unpacking payloads");
    let payloads = unpack_payloads_series_v2(&payloads);

    println!("aggregating payloads");
    let series = aggregate_normalize_series_metrics(&payloads);

    common_assertions(&series);

    println!("{:?}", series.keys());

    series
}

#[tokio::test]
async fn validate() {
    trace_init();

    // TODO need to see if can configure the agent flush interval
    std::thread::sleep(std::time::Duration::from_secs(30));

    println!("==== getting series data from agent-only pipeline ==== ");
    let agent_series = get_series_from_pipeline(fake_intake_agent_address()).await;

    println!("==== getting series data from agent-vector pipeline ====");
    let vector_series = get_series_from_pipeline(fake_intake_vector_address()).await;

    assert_eq!(agent_series, vector_series);

    // std::thread::sleep(std::time::Duration::from_secs(90));
}
