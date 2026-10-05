# OpenTelemetry Demo trace validation

This manual E2E test runs the multilingual
[OpenTelemetry Demo 3.1.0](https://github.com/open-telemetry/opentelemetry-demo/tree/3.1.0)
through Vector using the existing `vdev` Compose harness and Collector 0.159.0.

## Input and destination

```text
Demo applications → ingress Collector → Vector → destination Collector
                        input.jsonl                  output.jsonl
```

[scenario.py](data/scenario.py) runs one instrumented Locust user through three
sets of ad, recommendation, cart, and multi-item checkout requests, followed by a
missing-product request to generate errors. Checkout also exercises the Kafka
accounting and fraud-detection consumers. Browser traffic is disabled.

The ingress Collector accepts the applications' OTLP/gRPC and OTLP/HTTP telemetry.
Its [configuration](data/collector-source.yaml) captures traces in `input.jsonl`.
The [test matrix](config/test.yaml) runs two separate environments: `otlphttp`
forwards over HTTP to `http://vector:4318/v1/traces`, and `otlp` forwards over
gRPC to `vector:4317`. Each run forwards the input once and applies the same
validations; captures are kept separately.
Application logs and metrics are accepted by the Collector but not sent to Vector.

Vector runs [data/vector.yaml](data/vector.yaml), mounted as
`/etc/vector/vector.yaml`. Its `opentelemetry` source uses
`use_otlp_decoding: true`; its `opentelemetry` sink reads `otel.traces`, encodes
OTLP, and sends HTTP requests to `http://otel-collector-sink:4318/v1/traces`,
with a one-second batch timeout. There are no transforms.

The destination is a second local OpenTelemetry Collector, configured by
[collector-sink.yaml](data/collector-sink.yaml) to write `output.jsonl`.
No external telemetry backend is involved.

## Validations

The [Rust test](../opentelemetry/demo/mod.rs) verifies:

- Workload completion and drain: requires the completion marker, lets Kafka
  consumers finish, stops producers and the ingress Collector, and waits up to
  120 seconds for Vector's output before closing the destination capture.
- Service coverage: requires spans from ad, cart, checkout, currency, email,
  frontend, payment, product-catalog, quote, recommendation, shipping,
  load-generator, accounting, and fraud-detection.
- Trace coverage: requires roots, multi-span traces, children with captured
  parents, cross-service parent relationships, and error spans.
- Round-trip preservation: compares every span with its resource, scope, and
  schema URLs, including identities, parents, attributes, timestamps, status,
  events, links, flags, and dropped counts. Missing, extra, or changed spans,
  including changes to duplicate counts, fail. Only batching, span traversal
  order, and attribute-map order may differ.
- Capture support: rejects malformed IDs and unknown protobuf fields rather than
  silently discarding them before comparison.

Events, links, and scope groups containing multiple trace IDs are reported as
observed; they are not required coverage gates. This currently validates the
existing OTLP relay, not the proposed typed trace conversion. The flattened
`use_otlp_decoding: false` layout needs separate converter assertions because it
already loses scope information and cannot feed the current OTLP sink directly.

Once OTLP-to-Datadog conversion is implemented, the same input can also feed a
`datadog_traces` sink targeting a local intake receiver. That branch should check
the [RFC's cross-format mapping](../../../rfcs/2026-04-29-25329-trace-data-model/datadog-mapping.md#cross-format-conformance-otlp---vector---datadog_traces),
while the OTLP branch continues checking round-trip preservation.

## Running and results

Locally, with Docker available, run both ingress transports:

```shell
cargo vdev e2e run opentelemetry-demo --retries 0 --always-show-logs
```

To run one transport, add `-e 0.159.0-otlphttp` (HTTP) or `-e 0.159.0-otlp` (gRPC).

In CI, a Vector-team member submits a PR review starting with
`/ci-run-e2e-opentelemetry-demo`. The dedicated job tests the reviewed commit.
Ordinary comments, both “run all” commands, automatic PR CI, and schedules do not
run the demo. The candidate branch must include this test.

Captures, `coverage.json`, `differences.txt`, and `summary.md` are stored in the
`opentelemetry_demo_evidence` Docker volume, mounted under
`/output/opentelemetry-demo/`, in separate `otlphttp/` and `otlp/` directories.
The dedicated volume retains both runs across the harness's per-environment
cleanup. Each selected run replaces its own captures; unselected or unstarted
transports may retain older local results. CI publishes
these with the configuration, commit SHA, and runner logs in an
`otel-demo-<commit>` artifact and adds the summary to the job. Startup or drain
failures may leave partial evidence.
