use std::collections::{btree_map::Entry, BTreeMap};

#[allow(warnings, clippy::pedantic, clippy::nursery)]
mod ddmetric_proto {
    include!(concat!(env!("OUT_DIR"), "/datadog.agentpayload.rs"));
}

use ddmetric_proto::{sketch_payload::Sketch, SketchPayload};
use tracing::info;

use super::*;

const SKETCHES_ENDPOINT: &str = "/api/beta/sketches";

// TODO this needs a re-work to align with the SeriesIntake model in series.rs

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

            if let Entry::Vacant(e) = aggregate.entry(ctx) {
                e.insert(sketch.clone());
                info!("{:?}", sketch);
            }
        }
    }

    aggregate
}

// runs assertions that each set of payloads should be true to regardless
// of the pipeline
fn common_sketch_assertions(sketches: &BTreeMap<SketchContext, Sketch>) {
    // we should have received some metrics from the emitter
    assert!(!sketches.is_empty());
    info!("metric sketch received: {}", sketches.len());

    let mut found = false;
    sketches.keys().for_each(|ctx| {
        if ctx.metric_name.starts_with("foo_metric.distribution") {
            found = true;
        }
    });

    assert!(found, "Didn't receive metric type distribution");
}

async fn get_sketches_from_pipeline(address: String) -> BTreeMap<SketchContext, Sketch> {
    info!("getting sketch payloads");
    let payloads =
        get_fakeintake_payloads::<FakeIntakeResponseRaw>(&address, SKETCHES_ENDPOINT).await;

    info!("unpacking payloads");
    let mut payloads = unpack_proto_payloads(&payloads);

    info!("aggregating payloads");
    let sketches = aggregate_normalize_sketches(&mut payloads);

    common_sketch_assertions(&sketches);

    info!("{:?}", sketches.keys());

    sketches
}

pub(super) async fn validate() {
    info!("==== getting sketch data from agent-only pipeline ==== ");
    let agent_sketches = get_sketches_from_pipeline(fake_intake_agent_address()).await;

    info!("==== getting sketch data from agent-vector pipeline ====");
    let vector_sketches = get_sketches_from_pipeline(fake_intake_vector_address()).await;

    assert_eq!(agent_sketches, vector_sketches);
}
