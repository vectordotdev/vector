---
title: Highlights - July 2026
short: Highlights - July 2026
description: A tour of major features shipped since our last highlights post, plus CI improvements for external contributors.
authors: [ "pront" ]
date: "2026-07-02"
badges:
  type: announcement
  domains: [ "dev" ]
tags: [ "features", "dev", "ci", "guides" ]
---

_It has been a while since our [February 2025 highlights]({{< ref "/blog/highlights-february-2025" >}}). In this post we cover the most impactful features shipped across `0.46` through `0.56`, and a set of CI improvements that make life easier for external contributors._

## Features

### New components

| Component | Kind | Description | Author | PR |
| --- | --- | --- | --- | --- |
| [`mqtt`]({{< ref "/docs/reference/configuration/sources/mqtt" >}}) | Source | Ingest from MQTT brokers | [@StormStake](https://github.com/StormStake) | [#22752](https://github.com/vectordotdev/vector/pull/22752) |
| [`okta`]({{< ref "/docs/reference/configuration/sources/okta" >}}) | Source | Consume Okta system logs | [@sonnens](https://github.com/sonnens) | [#22968](https://github.com/vectordotdev/vector/pull/22968) |
| [`websocket`]({{< ref "/docs/reference/configuration/sources/websocket" >}}) | Source | Real-time ingestion from WebSocket APIs | [@benjamin-awd](https://github.com/benjamin-awd) | [#23449](https://github.com/vectordotdev/vector/pull/23449) |
| [`windows_event_log`]({{< ref "/docs/reference/configuration/sources/windows_event_log" >}}) | Source | Native Windows Event Log with bookmark-based checkpointing | [@tot19](https://github.com/tot19) | [#24305](https://github.com/vectordotdev/vector/pull/24305) |
| [`delay`]({{< ref "/docs/reference/configuration/transforms/delay" >}}) | Transform | Delay events by a fixed duration or VRL condition | [@esensar](https://github.com/esensar), [@Quad9DNS](https://github.com/Quad9DNS) | [#25407](https://github.com/vectordotdev/vector/pull/25407) |
| [`incremental_to_absolute`]({{< ref "/docs/reference/configuration/transforms/incremental_to_absolute" >}}) | Transform | Reconstruct absolute metrics from incremental data | [@GreyLilac09](https://github.com/GreyLilac09) | [#23374](https://github.com/vectordotdev/vector/pull/23374) |
| [`trace_to_log`]({{< ref "/docs/reference/configuration/transforms/trace_to_log" >}}) | Transform | Convert traces to logs | [@spencerho777](https://github.com/spencerho777) | [#24168](https://github.com/vectordotdev/vector/pull/24168) |
| [`window`]({{< ref "/docs/reference/configuration/transforms/window" >}}) | Transform | Sliding-window ring-buffer for noise reduction | [@ilinas](https://github.com/ilinas) | [#22609](https://github.com/vectordotdev/vector/pull/22609) |
| [`azure_logs_ingestion`]({{< ref "/docs/reference/configuration/sinks/azure_logs_ingestion" >}}) | Sink | Send logs to Azure Monitor via the Logs Ingestion API | [@jlaundry](https://github.com/jlaundry) | [#22912](https://github.com/vectordotdev/vector/pull/22912) |
| [`databricks_zerobus`]({{< ref "/docs/reference/configuration/sinks/databricks_zerobus" >}}) | Sink | Stream to Databricks Unity Catalog via Zerobus | [@flaviofcruz](https://github.com/flaviofcruz) | [#24840](https://github.com/vectordotdev/vector/pull/24840) |
| [`doris`]({{< ref "/docs/reference/configuration/sinks/doris" >}}) | Sink | Apache Doris via the Stream Load API | [@bingquanzhao](https://github.com/bingquanzhao) | [#23117](https://github.com/vectordotdev/vector/pull/23117) |
| [`postgres`]({{< ref "/docs/reference/configuration/sinks/postgres" >}}) | Sink | Send logs, metrics, and traces to Postgres | [@jorgehermo9](https://github.com/jorgehermo9) | [#21248](https://github.com/vectordotdev/vector/pull/21248) |
| `otlp` | Codec | Bidirectional Vector <-> OTLP conversion (logs and traces) | [@pront](https://github.com/pront) | [#24003](https://github.com/vectordotdev/vector/pull/24003) |
| `syslog` | Encoder | Encode Vector events as syslog (RFC5424 and RFC3164) | [@vparfonov](https://github.com/vparfonov) | [#23777](https://github.com/vectordotdev/vector/pull/23777) |
| `varint_length_delimited` | Framer | Varint length-delimited framing for protobuf streaming (ClickHouse-compatible) | [@modev2301](https://github.com/modev2301) | [#23352](https://github.com/vectordotdev/vector/pull/23352) |

### Source improvements

* The `opentelemetry` source gained metrics ingestion and now performs full OTLP decoding for logs, metrics, and traces, removing the need for complex remap steps in OTEL -> Vector -> OTEL pipelines.
* The `docker_logs` source retries Docker daemon communication failures with exponential backoff instead of giving up on transient hiccups.
* A performance regression that inflated CPU usage in the `file` and `kubernetes_logs` sources (introduced in `0.50.0`) was found and fixed.

### Transform improvements

* The `tag_cardinality_limit` transform gained several new controls: per-tag cardinality overrides (`per_tag_limits`), per-metric tracking isolation (`tracking_scope: per_metric`), a global key cap (`max_tracked_keys`), and the ability to opt entire metrics out of cardinality tracking.
* The `syslog` encoding transform gained improved RFC compliance, support for scalars, nested objects, and arrays in structured data, and better UTF-8 safety.

### Sink improvements

* A configurable `retry_strategy` for HTTP-based sinks gives users control over which response codes are retried (`default` / `none` / `all` / `custom`).
* The `aws_s3` sink gained Apache Parquet batch encoding.
* The `datadog_metrics` sink switched to the Series v2 endpoint with zstd compression by default, and `datadog_logs` switched its default to `zstd` as well.

### Operations and observability

* `--watch-config` now also watches enrichment table files.
* `vector top` gained scrollable, sortable, and filterable views (press `?` for keybinds).
* Unit tests support an `expected_event_count` field on outputs to assert on emitted event counts.
* Task-transform `utilization` no longer counts downstream wait time, giving a more accurate saturation view.
* New internal metrics for capacity planning and backpressure detection:
  * `source_buffer_max_size_bytes`, `source_buffer_max_size_events`
  * `transform_buffer_max_size_bytes`, `transform_buffer_max_size_events`
  * `source_buffer_utilization_mean`, `transform_buffer_utilization_mean` (EWMA-smoothed)
  * `component_latency_seconds` (histogram), `component_latency_mean_seconds` (gauge)
  * `source_send_latency_seconds`, `source_send_batch_latency_seconds`

For the complete list of changes, breaking changes, and upgrade steps, see the [releases page]({{< ref "/releases" >}}).

### VRL

#### Features

##### New functions

| Function | Author | PR |
| --- | --- | --- |
| [`aggregate_vector_metrics`]({{< ref "/docs/reference/vrl/functions/#aggregate_vector_metrics" >}}) | [@esensar](https://github.com/esensar), [@Quad9DNS](https://github.com/Quad9DNS) | [vector#23430](https://github.com/vectordotdev/vector/pull/23430) |
| [`basename`]({{< ref "/docs/reference/vrl/functions/#basename" >}}) | [@titaneric](https://github.com/titaneric) | [vrl#1531](https://github.com/vectordotdev/vrl/pull/1531) |
| [`decode_lz4`]({{< ref "/docs/reference/vrl/functions/#decode_lz4" >}}) | [@jimmystewpot](https://github.com/jimmystewpot) | [vrl#1339](https://github.com/vectordotdev/vrl/pull/1339) |
| [`dirname`]({{< ref "/docs/reference/vrl/functions/#dirname" >}}) | [@titaneric](https://github.com/titaneric) | [vrl#1532](https://github.com/vectordotdev/vrl/pull/1532) |
| [`encode_csv`]({{< ref "/docs/reference/vrl/functions/#encode_csv" >}}) | [@armleth](https://github.com/armleth) | [vrl#1649](https://github.com/vectordotdev/vrl/pull/1649) |
| [`encode_lz4`]({{< ref "/docs/reference/vrl/functions/#encode_lz4" >}}) | [@jimmystewpot](https://github.com/jimmystewpot) | [vrl#1339](https://github.com/vectordotdev/vrl/pull/1339) |
| [`find_vector_metrics`]({{< ref "/docs/reference/vrl/functions/#find_vector_metrics" >}}) | [@esensar](https://github.com/esensar), [@Quad9DNS](https://github.com/Quad9DNS) | [vector#23430](https://github.com/vectordotdev/vector/pull/23430) |
| [`from_entries`]({{< ref "/docs/reference/vrl/functions/#from_entries" >}}) | [@close2code-palm](https://github.com/close2code-palm) | [vrl#1653](https://github.com/vectordotdev/vrl/pull/1653) |
| [`get_vector_metric`]({{< ref "/docs/reference/vrl/functions/#get_vector_metric" >}}) | [@esensar](https://github.com/esensar), [@Quad9DNS](https://github.com/Quad9DNS) | [vector#23430](https://github.com/vectordotdev/vector/pull/23430) |
| [`haversine`]({{< ref "/docs/reference/vrl/functions/#haversine" >}}) | [@esensar](https://github.com/esensar), [@Quad9DNS](https://github.com/Quad9DNS) | [vrl#1442](https://github.com/vectordotdev/vrl/pull/1442) |
| [`http_request`]({{< ref "/docs/reference/vrl/functions/#http_request" >}}) | [@benjamin-awd](https://github.com/benjamin-awd) | [vrl#1360](https://github.com/vectordotdev/vrl/pull/1360) |
| [`decrypt_ip`]({{< ref "/docs/reference/vrl/functions/#decrypt_ip" >}}) | [@alterstep](https://github.com/alterstep) | [vrl#1506](https://github.com/vectordotdev/vrl/pull/1506) |
| [`encrypt_ip`]({{< ref "/docs/reference/vrl/functions/#encrypt_ip" >}}) | [@alterstep](https://github.com/alterstep) | [vrl#1506](https://github.com/vectordotdev/vrl/pull/1506) |
| [`parse_yaml`]({{< ref "/docs/reference/vrl/functions/#parse_yaml" >}}) | [@juchem](https://github.com/juchem) | [vrl#1602](https://github.com/vectordotdev/vrl/pull/1602) |
| [`pop`]({{< ref "/docs/reference/vrl/functions/#pop" >}}) | [@jlambatl](https://github.com/jlambatl) | [vrl#1501](https://github.com/vectordotdev/vrl/pull/1501) |
| [`split_path`]({{< ref "/docs/reference/vrl/functions/#split_path" >}}) | [@titaneric](https://github.com/titaneric) | [vrl#1533](https://github.com/vectordotdev/vrl/pull/1533) |
| [`to_entries`]({{< ref "/docs/reference/vrl/functions/#to_entries" >}}) | [@close2code-palm](https://github.com/close2code-palm) | [vrl#1653](https://github.com/vectordotdev/vrl/pull/1653) |
| [`xxhash`]({{< ref "/docs/reference/vrl/functions/#xxhash" >}}) | [@stigglor](https://github.com/stigglor) | [vrl#1473](https://github.com/vectordotdev/vrl/pull/1473) |

##### Syntax

* `else` and `else if` can now appear on a new line after the closing `}` of an `if` block. Previously the newline terminated the expression, forcing `} else if {` on a single line.
* String literals now support `\u{HEX}` Unicode escape sequences (`"hello\u{1F30E}world"`). Invalid sequences (empty braces, non-hex digits, surrogate codepoints, or values above U+10FFFF) fail at compile time with a specific error.

##### Type system and function surface

* `find` now returns `null` when no match is found, instead of `-1`. Audit existing programs that branch on `find(...) < 0`.
* Vector-specific VRL functions are now available in the standalone VRL CLI (`vector vrl`) and in codec VRL transforms, closing a long-standing surface gap.
* Enrichment functions gained bounded date range filtering (`from` / `to`) and wildcard match support.
* `encode_proto` gained looser scalar coercion: integers and strings are accepted for `bool` fields, integers for `float` and `double` fields, and integer or boolean map keys are stringified per the protobuf JSON mapping. A new `allow_lossy_string_coercion` flag lets strict callers opt back into spec-only encoding.

##### Performance

* `encode_gzip` / `decode_gzip` / `encode_zlib` / `decode_zlib` switched to the [zlib-rs](https://github.com/trifectatechfoundation/zlib-rs) backend for significantly faster compression and decompression.
* `encode_base64` / `decode_base64` / `decode_mime_q` moved to a SIMD backend.
* `parse_regex_all` reuses the compiled regex across invocations.

##### VRL in more places

* HTTP client sources accept VRL expressions in query parameters and in the request body, enabling dynamic request construction (e.g. embedding `now()` or environment variables in outgoing requests).
* Custom auth strategies expose the client address and URL path to VRL scripts, and can now write scalar values back into the auth context via `%field = value` writes.
* Templating landed on the `http` sink's `uri` and `request.headers` fields.

##### Live reload

* `--watch-config` also watches external VRL files referenced by `remap` transforms.
* Vector reloads external VRL files on `SIGHUP`.

##### VRL Playground

The [VRL Playground](https://playground.vrl.dev) gained a timezone selector, performance timing display, output-panel line wrap, and a series of dropdown and rendering fixes.

#### Fixes

* The compiler now reports **every** unhandled-error in a single compilation pass instead of stopping at the first. Fix all your fallible calls in one go rather than a compile-fix-compile loop.
* Fallible-call error location is now correct. Previously, a missing `!` or `, err =` on an earlier call could cause the diagnostic to point at a later, unrelated assignment. Now the error is reported on the actual fallible expression, including inside `for_each` and `map_values` closures.
* False positive in the unused-variable diagnostic (`E900`) fixed. A variable used before being reassigned (shadowed) is no longer flagged as unused at its original assignment.
* Lexer errors surface specific error codes (e.g. `E209 invalid escape character`) instead of the generic `E202 syntax error`, and their spans now point at the exact character rather than the whole call.
* `vector test` output honors `--color {auto|always|never}` and `VECTOR_COLOR`; VRL diagnostics stop emitting stray ANSI escape sequences when color is disabled or when running non-interactively.

## CI and developer ergonomics

A recurring source of friction for contributors was hitting CI failures that were hard to reproduce locally or caused by false positives. Several checks were replaced or improved to reduce that gap. We also fixed long-standing flaky tests.

All of the following checks can be run locally before pushing, so there are no surprises on the PR.

* **Simpler PR title check.** The semantic PR title action was replaced with a small inline script. Contributors are no longer blocked by a 180-entry hardcoded scope allowlist that drifted every time a component was added or renamed.
* **`typos` replaces `check-spelling`.** The old `check-spelling` workflow produced enough false positives to be a constant source of friction on PRs. It has been replaced with [`typos`](https://github.com/crate-ci/typos), a Rust-native spell checker that understands identifiers and hex literals natively. Run it locally with `cargo binstall typos-cli && typos`.
* **YAML/JSON/TS formatting.** Prettier formatting checks now run in CI for YAML, JSON, and TypeScript files. Run `make check-prettier` locally and `make fix-prettier` to auto-fix.
* **Improved Markdown checks.** The `markdownlint` configuration was tightened to catch more common style issues. Run `make check-markdown` locally and `make fix-markdown` to auto-fix.

## Thank you

Vector genuinely would not be where it is without its community.

We are very happy to notice growing interest in Vector:

| Year | Unique | New | Returning |
| --- | --- | --- | --- |
| 2024 | 177 | 130 | 47 |
| 2025 | 191 | 144 | 47 |
| 2026 (YTD, 5 months) | 133 | 90 | 43 |

Thank you to every PR contributor behind those numbers, and to everyone who opened an issue, reviewed code, improved docs, or started a discussion.

We look forward to the future.

## Appendix

### Vector releases in this window

* [`0.46.0`]({{< ref "/releases/0.46.0" >}}) — 2025-04-04
* [`0.47.0`]({{< ref "/releases/0.47.0" >}}) — 2025-05-20
* [`0.48.0`]({{< ref "/releases/0.48.0" >}}) — 2025-06-30
* [`0.49.0`]({{< ref "/releases/0.49.0" >}}) — 2025-08-12
* [`0.50.0`]({{< ref "/releases/0.50.0" >}}) — 2025-09-23
* [`0.51.0`]({{< ref "/releases/0.51.0" >}}) — 2025-11-04
* [`0.52.0`]({{< ref "/releases/0.52.0" >}}) — 2025-12-16
* [`0.53.0`]({{< ref "/releases/0.53.0" >}}) — 2026-01-27
* [`0.54.0`]({{< ref "/releases/0.54.0" >}}) — 2026-03-10
* [`0.55.0`]({{< ref "/releases/0.55.0" >}}) — 2026-04-22
* [`0.56.0`]({{< ref "/releases/0.56.0" >}}) — 2026-06-03

### VRL

* [VRL CHANGELOG](https://github.com/vectordotdev/vrl/blob/main/CHANGELOG.md)
* [VRL function reference]({{< ref "/docs/reference/vrl/functions" >}})
* [VRL Playground](https://playground.vrl.dev)
