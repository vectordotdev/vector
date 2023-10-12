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
    in_payloads: &Vec<FakeIntakePayload>,
) -> Vec<ddmetric_proto::MetricPayload> {
    let mut out_payloads = vec![];

    in_payloads.iter().for_each(|payload| {
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
        // println!("{:?} {:?}", _ctx, series);
    }

    aggregate
}

// runs assertions that each set of payloads should be true to regardless
// of the pipeline
fn common_assertions(payloads: &Vec<ddmetric_proto::MetricPayload>) {
    assert!(payloads.len() > 0);
    println!("metric payloads received: {}", payloads.len());
}

#[tokio::test]
async fn validate() {
    trace_init();

    // TODO need to see if can configure the agent flush interval
    std::thread::sleep(std::time::Duration::from_secs(30));

    println!("getting payloads from agent-only pipeline");
    let agent_payloads = get_payloads_agent(SERIES_ENDPOINT).await;

    println!("unpacking payloads from agent-only pipeline");
    let agent_payloads = unpack_payloads_series_v2(&agent_payloads);
    common_assertions(&agent_payloads);

    println!("aggregating payloads from agent-only pipeline");
    let agent_payloads = aggregate_normalize_series_metrics(&agent_payloads);

    println!("{:?}", agent_payloads.keys());

    // let foo_rate_agent = agent_payloads.get("foo_metric.rate");
    // println!("AGENT RATE AGGREGATE: {:?}", foo_rate_agent);

    println!("getting log payloads from agent-vector pipeline");
    let vector_payloads = get_payloads_vector(SERIES_ENDPOINT).await;

    println!("unpacking payloads from agent-vector pipeline");
    let vector_payloads = unpack_payloads_series_v2(&vector_payloads);
    common_assertions(&vector_payloads);

    println!("aggregating payloads from agent-vector pipeline");
    let vector_payloads = aggregate_normalize_series_metrics(&vector_payloads);
    println!("{:?}", vector_payloads.keys());

    assert_eq!(agent_payloads, vector_payloads);

    // std::thread::sleep(std::time::Duration::from_secs(90));
}
