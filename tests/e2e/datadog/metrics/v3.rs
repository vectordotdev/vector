//! Structural validation of the Datadog metrics sink's V3 columnar protobuf format.
//!
//! The real Agent *does* speak V3 for series (Agent 7.81+ defaults series submission to
//! `/api/intake/metrics/v3/series`), but it picks one wire format per metric type and sends it
//! identically to `dd_url` *and* every `additional_endpoints` entry — there's no per-destination
//! negotiation. Left alone, that means an Agent new enough to speak V3 would send V3 to Vector
//! too, which 404s: Vector's `datadog_agent` source has no V3 *ingestion* route (only the sink
//! encodes V3; nothing decodes it). That's also why the V1/V2 suite (`v1v2::series`, a sibling
//! of this module) pins its Agent version below 7.81 — it can't handle the V3 request either.
//!
//! This suite instead uses the Agent's `use_v3_api.series.endpoints` config (see
//! `../../datadog-metrics-v3/data/agent.yaml`) to force V3 specifically to `dd_url`
//! (fakeintake-agent) while forcing V2 to the `additional_endpoints` entry (Vector) — the only
//! format its source can ingest. Vector then re-encodes what it ingested to V3 on its way to
//! fakeintake-vector. Both fakeintake instances end up holding V3 series data computed by two
//! different encoders (the real Agent's, and Vector's) from the *same* underlying dogstatsd
//! traffic and flush window, which is exactly the "single shared Agent" trick `v1v2::series`
//! uses for V1/V2 — `validate_series` below reuses that module's `SeriesContext`/`TimeBucket`/
//! `generate_series_intake`/`common_series_assertions`/`compare_intakes` directly (via
//! `super::v1v2::series`), converting our decoded V3 shape into the same `MetricSeries`/
//! `MetricPayload` those helpers already operate on rather than reimplementing series comparison
//! from scratch. (`v1v2` and `v3` are separate sibling modules — not nested — so that each
//! suite's e2e test_filter can target one exactly, without a substring match picking up the
//! other's tests too; see the `test_filter` comments in each suite's `config/test.yaml`.)
//!
//! Series payloads are decoded by asking fakeintake to do it (`?format=json`, backed by
//! `ParseMetricSeriesV3` in datadog-agent's `test/fakeintake/server/serverstore/parser.go`)
//! rather than parsing the wire format here — fakeintake's own decoder is what actually proves
//! the wire format is correct; re-decoding it locally would just be testing our encoder against
//! our own decoder.
//!
//! fakeintake has no equivalent decoder for the V3 *sketches* route: as of
//! `docker.io/datadog/fakeintake:latest`, `/api/intake/metrics/v3/sketches` isn't in that
//! parser map at all, and `?format=json` 400s for it. Sketches also have no real-Agent baseline
//! to diff against regardless — dogstatsd histograms/distributions still go out via the Agent's
//! old `/api/beta/sketches` route even with `use_v3_api` forcing V3 for series, so there's no
//! Agent-native V3 sketch payload to compare to in the first place. So sketches are still decoded
//! locally below via a minimal columnar-format reader and checked structurally only — tag/resource
//! dictionaries and sketch bins are intentionally not reconstructed. (The `datadog-agent-metrics-v3`
//! crate the sink's encoder uses only exposes encode-side types — `V3Writer`/`V3MetricBuilder` —
//! no decoder, so there's nothing to reuse from there either.) Revisit both limitations if
//! fakeintake grows a V3 sketches parser and/or the Agent grows V3 sketch submission.

use std::collections::HashMap;

use async_compression::tokio::bufread::ZstdDecoder;
use base64::{Engine, prelude::BASE64_STANDARD};
use serde::Deserialize;
use tokio::io::AsyncReadExt;
use tracing::info;
use vector::test_util::trace_init;

use super::v1v2::series::{MetricPayload, MetricPoint, MetricSeries, SeriesIntake};
// Brings in `get_fakeintake_payloads`, `FakeIntakeResponse{Json,Raw}`, `FakeIntakePayloadJson`,
// `fake_intake_agent_address`/`fake_intake_vector_address` — all already accessible here (this
// module is a descendant of where they're defined), same glob-import `v1v2` itself uses to reach
// the same `datadog`-level helpers.
use super::*;

const SERIES_ENDPOINT_V3: &str = "/api/intake/metrics/v3/series";
const SKETCHES_ENDPOINT_V3: &str = "/api/intake/metrics/v3/sketches";

// The intake API wraps the columnar `MetricData` message as field 3 (`metricData`) of an outer
// `Payload` envelope (see `intake_v3.proto`); without unwrapping this, the bytes don't parse.
const PAYLOAD_METRIC_DATA_FIELD: u32 = 3;

// Field numbers from `intake_v3.proto`, mirrored in `datadog_agent_metrics_v3::constants`.
const DICT_NAME_STR_FIELD: u32 = 1;
const TYPES_FIELD: u32 = 10;
const NAMES_FIELD: u32 = 11;
const NUM_POINTS_FIELD: u32 = 15;

// ── Series: decoded by fakeintake itself, diffed against the real Agent baseline ────────────

/// The shape fakeintake's `ParseMetricSeriesV3` decodes a V3 series metric into, as seen through
/// its `?format=json` API. Deliberately separate from `MetricSeries` (mirrors how `series`
/// itself decodes V1 JSON into `DatadogSeriesMetric` before converting) since that's a prost
/// type with no `Deserialize` impl.
#[derive(Deserialize, Debug)]
struct V3SeriesJson {
    metric: String,
    r#type: i32,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    interval: i64,
    #[serde(default)]
    points: Vec<V3PointJson>,
}

#[derive(Deserialize, Debug)]
struct V3PointJson {
    #[serde(default)]
    value: f64,
    timestamp: i64,
}

impl From<&V3SeriesJson> for MetricSeries {
    fn from(input: &V3SeriesJson) -> Self {
        MetricSeries {
            resources: vec![],
            metric: input.metric.clone(),
            tags: input.tags.clone(),
            points: input
                .points
                .iter()
                .map(|p| MetricPoint {
                    value: p.value,
                    timestamp: p.timestamp,
                })
                .collect(),
            r#type: input.r#type,
            unit: String::new(),
            source_type_name: String::new(),
            interval: input.interval,
            metadata: None,
        }
    }
}

fn unpack_v3_series(payloads: &[FakeIntakePayloadJson]) -> Vec<MetricSeries> {
    payloads
        .iter()
        .flat_map(|payload| {
            payload
                .data
                .as_array()
                .expect("V3 series JSON payload data should be an array")
                .iter()
                .map(|serie| {
                    let parsed: V3SeriesJson = serde_json::from_value(serie.clone())
                        .expect("Failed to parse fakeintake's decoded V3 series JSON");
                    MetricSeries::from(&parsed)
                })
        })
        .collect()
}

async fn get_v3_series_from_pipeline(address: String) -> SeriesIntake {
    info!("getting V3 series payloads");
    let payloads =
        get_fakeintake_payloads::<FakeIntakeResponseJson>(&address, SERIES_ENDPOINT_V3).await;

    info!("unpacking payloads");
    let series = unpack_v3_series(&payloads.payloads);
    let payloads = vec![MetricPayload { series }];

    info!("generating series intake");
    let intake = super::v1v2::series::generate_series_intake(&payloads);

    super::v1v2::series::common_series_assertions(&intake);

    info!("{intake:?}");

    intake
}

async fn validate_series() {
    info!("==== getting V3 series data from the real Agent baseline ====");
    let agent_intake = get_v3_series_from_pipeline(fake_intake_agent_address()).await;

    info!("==== getting V3 series data from Vector's re-encode ====");
    let vector_intake = get_v3_series_from_pipeline(fake_intake_vector_address()).await;

    super::v1v2::series::compare_intakes(&agent_intake, &vector_intake);
}

// ── Sketches: no fakeintake decoder exists yet, so decode the wire format here ─────────────

// V3 payloads are always zstd-compressed, unlike V1 (deflate) and V2 (zstd, but detected
// dynamically by `v1v2::decompress_payload` since it also has to handle V1's zlib payloads) —
// so this module gets its own always-zstd decompressor rather than reaching into `v1v2` for
// its more general one; the only thing deliberately shared across the `v1v2`/`v3` split is the
// series comparison logic above.
async fn zstd_decompress(payload: &[u8]) -> Vec<u8> {
    let mut decompressor = ZstdDecoder::new(payload);
    let mut decompressed = Vec::new();
    decompressor
        .read_to_end(&mut decompressed)
        .await
        .expect("V3 payloads are always zstd-compressed");
    decompressed
}

fn read_varint(buf: &[u8], pos: &mut usize) -> u64 {
    let mut result = 0u64;
    let mut shift = 0;
    loop {
        let byte = buf[*pos];
        *pos += 1;
        result |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    result
}

fn zigzag_decode(v: u64) -> i64 {
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}

/// Walks the top-level fields of a protobuf message, returning the raw bytes of each
/// length-delimited (wire type 2) field by field number. Sufficient for V3's `MetricData` and
/// `Payload` messages, whose fields of interest are all length-delimited (bytes, or
/// packed-repeated); other wire types are skipped so parsing stays correct even for fields this
/// module doesn't care about.
fn read_length_delimited_fields(buf: &[u8]) -> HashMap<u32, Vec<u8>> {
    let mut fields = HashMap::new();
    let mut pos = 0;
    while pos < buf.len() {
        let tag = read_varint(buf, &mut pos);
        let field = (tag >> 3) as u32;
        let wire = tag & 0x7;
        match wire {
            0 => {
                read_varint(buf, &mut pos);
            }
            1 => pos += 8,
            2 => {
                let len = read_varint(buf, &mut pos) as usize;
                fields.insert(field, buf[pos..pos + len].to_vec());
                pos += len;
            }
            5 => pos += 4,
            _ => panic!("unexpected protobuf wire type {wire} at offset {pos}"),
        }
    }
    fields
}

/// Field 1 (`DictNameStr`): varint-length-prefixed UTF-8 strings, concatenated.
fn decode_name_dict(buf: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let len = read_varint(buf, &mut pos) as usize;
        names.push(String::from_utf8_lossy(&buf[pos..pos + len]).into_owned());
        pos += len;
    }
    names
}

fn decode_packed_varint(buf: &[u8]) -> Vec<u64> {
    let mut values = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        values.push(read_varint(buf, &mut pos));
    }
    values
}

fn decode_packed_zigzag(buf: &[u8]) -> Vec<i64> {
    decode_packed_varint(buf)
        .into_iter()
        .map(zigzag_decode)
        .collect()
}

/// Reverses the writer's delta encoding (see `datadog_agent_metrics_v3::writer::delta_encode`).
fn delta_decode(deltas: &[i64]) -> Vec<i64> {
    let mut out = Vec::with_capacity(deltas.len());
    let mut acc = 0i64;
    for &d in deltas {
        acc += d;
        out.push(acc);
    }
    out
}

/// The handful of V3 `MetricData` columns needed to prove sketch metrics arrived correctly
/// named and typed. Deliberately not a full decode: tag/resource dictionaries, point values,
/// and sketch bins are not reconstructed (see the module doc comment for why).
struct V3MetricData {
    name_dict: Vec<String>,
    types: Vec<u64>,
    name_ids: Vec<i64>,
    num_points: Vec<u64>,
}

impl V3MetricData {
    fn parse(metric_data: &[u8]) -> Self {
        let fields = read_length_delimited_fields(metric_data);

        let name_dict = fields
            .get(&DICT_NAME_STR_FIELD)
            .map(|b| decode_name_dict(b))
            .unwrap_or_default();
        let types = fields
            .get(&TYPES_FIELD)
            .map(|b| decode_packed_varint(b))
            .unwrap_or_default();
        let name_ids = fields
            .get(&NAMES_FIELD)
            .map(|b| delta_decode(&decode_packed_zigzag(b)))
            .unwrap_or_default();
        let num_points = fields
            .get(&NUM_POINTS_FIELD)
            .map(|b| decode_packed_varint(b))
            .unwrap_or_default();

        assert_eq!(
            types.len(),
            name_ids.len(),
            "TYPES/NAMES column length mismatch"
        );
        assert_eq!(
            types.len(),
            num_points.len(),
            "TYPES/NUM_POINTS column length mismatch"
        );

        Self {
            name_dict,
            types,
            name_ids,
            num_points,
        }
    }

    fn metric_count(&self) -> usize {
        self.types.len()
    }

    /// Resolves a (1-based; 0 = none) name-dictionary id back to its string.
    fn resolve_name(&self, id: i64) -> &str {
        usize::try_from(id - 1)
            .ok()
            .and_then(|idx| self.name_dict.get(idx))
            .map(String::as_str)
            .unwrap_or_else(|| {
                panic!(
                    "name id {id} did not resolve via the {}-entry name dictionary",
                    self.name_dict.len()
                )
            })
    }
}

async fn fetch_v3_sketch_data() -> Vec<V3MetricData> {
    let address = fake_intake_vector_address();
    let response =
        get_fakeintake_payloads::<FakeIntakeResponseRaw>(&address, SKETCHES_ENDPOINT_V3).await;

    let mut result = Vec::new();
    for payload in &response.payloads {
        let raw = BASE64_STANDARD
            .decode(&payload.data)
            .expect("fakeintake payload data is not valid base64");

        // Skip tiny diagnostic/health-check payloads (e.g. the Agent's `{}` diagnose probe).
        if raw.len() < 10 {
            continue;
        }

        let decompressed = zstd_decompress(&raw).await;
        let envelope_fields = read_length_delimited_fields(&decompressed);
        let metric_data = envelope_fields.get(&PAYLOAD_METRIC_DATA_FIELD).unwrap_or_else(|| {
            panic!(
                "V3 payload envelope has no metricData field ({PAYLOAD_METRIC_DATA_FIELD}); got fields {:?}",
                envelope_fields.keys().collect::<Vec<_>>()
            )
        });

        result.push(V3MetricData::parse(metric_data));
    }
    result
}

async fn validate_sketches() {
    info!(
        "==== getting V3 sketches data (decoded locally; fakeintake has no V3 sketches decoder) ===="
    );
    let payloads = fetch_v3_sketch_data().await;
    assert!(
        !payloads.is_empty(),
        "no V3 sketches payloads received at {SKETCHES_ENDPOINT_V3}"
    );

    let mut found_distribution = false;
    for data in &payloads {
        assert!(
            data.metric_count() > 0,
            "V3 sketches payload decoded with no metrics"
        );

        for i in 0..data.metric_count() {
            let metric_type = data.types[i] & 0x0F;
            assert_eq!(
                metric_type, 4,
                "found a non-sketch metric type {metric_type:#x} on the sketches endpoint"
            );
            assert!(data.num_points[i] > 0, "sketch metric has zero points");

            if data
                .resolve_name(data.name_ids[i])
                .starts_with("foo_metric.distribution")
            {
                found_distribution = true;
            }
        }
    }

    assert!(
        found_distribution,
        "didn't receive V3 sketch metric type distribution"
    );
}

#[tokio::test]
async fn validate() {
    trace_init();

    // Even with configuring docker service dependencies, we need a small buffer of time
    // to ensure events flow through to fakeintake before asking for them.
    std::thread::sleep(std::time::Duration::from_secs(2));

    validate_series().await;
    validate_sketches().await;
}
