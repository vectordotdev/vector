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

// Sums up the metrics in each series by name.
fn aggregate_series_metrics(
    payloads: &Vec<ddmetric_proto::MetricPayload>,
) -> BTreeMap<String, f64> {
    let mut aggregate = BTreeMap::new();

    for metric_payload in payloads {
        for serie in &metric_payload.series {
            // filter out the metrics we don't care about
            if !serie.metric.starts_with("foo_metric") {
                continue;
            }

            // TODO expand on this

            let interval = serie.interval as f64;
            match serie.r#type() {
                ddmetric_proto::metric_payload::MetricType::Unspecified => {
                    panic!("unspecified metric type")
                }
                ddmetric_proto::metric_payload::MetricType::Count => {
                    if let Some((t, v)) = aggregate.get_mut(&serie.metric) {
                        for point in &serie.points {
                            *t = point.timestamp;
                            *v += point.value;
                        }
                    } else {
                        for point in &serie.points {
                            aggregate.insert(serie.metric.clone(), (point.timestamp, point.value));
                        }
                    }
                }
                ddmetric_proto::metric_payload::MetricType::Rate => {
                    if let Some((t, v)) = aggregate.get_mut(&serie.metric) {
                        for point in &serie.points {
                            *v += point.value * interval;
                            *t = point.timestamp;
                        }
                    } else {
                        for (idx, point) in serie.points.iter().enumerate() {
                            if idx == 0 {
                                aggregate.insert(
                                    serie.metric.clone(),
                                    (point.timestamp, point.value * interval),
                                );
                            } else {
                                if let Some((t, v)) = aggregate.get_mut(&serie.metric) {
                                    *v += point.value * interval;
                                    *t = point.timestamp;
                                }
                            }
                        }
                    }
                }
                ddmetric_proto::metric_payload::MetricType::Gauge => {
                    // last one wins
                    if let Some(point) = serie.points.last() {
                        if let Some((t, v)) = aggregate.get_mut(&serie.metric) {
                            if point.timestamp > *t {
                                *t = point.timestamp;
                                *v = point.value;
                            }
                        } else {
                            aggregate.insert(serie.metric.clone(), (point.timestamp, point.value));
                        }
                    }
                }
            }
        }
    }

    // remove the timestamps
    let aggregate = aggregate.into_iter().map(|(k, v)| (k, v.1)).collect();

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
    let agent_payloads = aggregate_series_metrics(&agent_payloads);

    println!("{:?}", agent_payloads.keys());

    // let foo_rate_agent = agent_payloads.get("foo_metric.rate");
    // println!("AGENT RATE AGGREGATE: {:?}", foo_rate_agent);

    println!("getting log payloads from agent-vector pipeline");
    let vector_payloads = get_payloads_vector(SERIES_ENDPOINT).await;

    println!("unpacking payloads from agent-vector pipeline");
    let vector_payloads = unpack_payloads_series_v2(&vector_payloads);
    common_assertions(&vector_payloads);

    println!("aggregating payloads from agent-vector pipeline");
    let vector_payloads = aggregate_series_metrics(&vector_payloads);
    println!("{:?}", vector_payloads.keys());

    assert_eq!(agent_payloads, vector_payloads);

    // std::thread::sleep(std::time::Duration::from_secs(90));
}
