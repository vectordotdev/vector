use std::collections::BTreeMap;

#[allow(warnings, clippy::pedantic, clippy::nursery)]
mod ddmetric_proto {
    include!(concat!(env!("OUT_DIR"), "/datadog.agentpayload.rs"));
}

use ddmetric_proto::{sketch_payload::Sketch, SketchPayload};

use super::*;

const SKETCHES_ENDPOINT: &str = "/api/beta/sketches";

// unique identification of a Sketch
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
struct SketchContext {
    metric_name: String,
    tags: Vec<String>,
}

fn aggregate_normalize_sketches(
    payloads: &mut Vec<SketchPayload>,
) -> BTreeMap<SketchContext, Sketch> {
    let mut aggregate = BTreeMap::new();

    for payload in payloads {
        for sketch in &mut payload.sketches {
            // filter out the metrics we don't care about
            if !sketch.metric.starts_with("foo_metric") {
                continue;
            }

            let ctx = SketchContext {
                metric_name: sketch.metric.clone(),
                tags: sketch.tags.clone(),
            };

            if sketch.host.ends_with("-vector") {
                sketch.host.truncate(sketch.host.len() - "-vector".len());
            }

            if !aggregate.contains_key(&ctx) {
                aggregate.insert(ctx, sketch.clone());
                println!("{:?}", sketch);
            }
        }
    }

    aggregate
}

// runs assertions that each set of payloads should be true to regardless
// of the pipeline
fn common_sketch_assertions(sketches: &BTreeMap<SketchContext, Sketch>) {
    // we should have received some metrics from the emitter
    assert!(sketches.len() > 0);
    println!("metric sketch received: {}", sketches.len());

    // specifically we should have received each of these
    let mut found = vec![(false, "distribution")];
    sketches.keys().for_each(|ctx| {
        found.iter_mut().for_each(|found| {
            if ctx
                .metric_name
                .starts_with(&format!("foo_metric.{}", found.1))
            {
                println!("received {}", found.1);
                found.0 = true;
            }
        });
    });

    found
        .iter()
        .for_each(|(found, mtype)| assert!(found, "Didn't receive metric type {}", *mtype));
}

async fn get_sketches_from_pipeline(address: String) -> BTreeMap<SketchContext, Sketch> {
    println!("getting sketch payloads");
    let payloads =
        get_fakeintake_payloads::<FakeIntakeResponseRaw>(&address, SKETCHES_ENDPOINT).await;

    println!("unpacking payloads");
    let mut payloads = unpack_proto_payloads(&payloads);

    println!("aggregating payloads");
    let sketches = aggregate_normalize_sketches(&mut payloads);

    common_sketch_assertions(&sketches);

    println!("{:?}", sketches.keys());

    sketches
}

pub(super) async fn validate() {
    println!("==== getting sketch data from agent-only pipeline ==== ");
    let agent_sketches = get_sketches_from_pipeline(fake_intake_agent_address()).await;

    println!("==== getting sketch data from agent-vector pipeline ====");
    let vector_sketches = get_sketches_from_pipeline(fake_intake_vector_address()).await;

    assert_eq!(agent_sketches, vector_sketches);
}
