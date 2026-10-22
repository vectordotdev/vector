# Vector Rust Style

## Formatting

Use `make fmt` to format, `make check-fmt` to check formatting, and `make check-clippy` to lint.
Manually format macro contents that `rustfmt` cannot handle.

## Import Statements (`use`)

All `use` statements must be at the **top of the file/module** or at the top of `mod tests`.
This is for consistency.

**Correct:**

```rust
use std::time::Duration;

use governor::clock;

use crate::config::TransformConfig;

fn my_function() {
    // function code
}
```

**Incorrect:**

```rust
fn my_function() {
    use std::time::Duration;  // WRONG; Do not insert `use` inside functions
    // function code
}
```

**Organization:**

- Group imports: `std` → external crates → internal (`crate::`)

**Trait imports:**

When importing a trait only to use its methods, import it as `_`. This makes the intent explicit
without introducing a name that is otherwise unused:

```rust
use std::io::Write as _;

use futures::StreamExt as _;
```

Keep a named import when the trait is also referenced by name, such as in a trait bound, an `impl`
block, a trait object, or a fully qualified method call.

## Unit tests

Unit tests for a type live in the same source file as that type (`#[cfg(test)] mod tests` at
the bottom of the file) unless they exercise behavior that spans multiple modules. Cross-module
tests belong with the composing type, not in a catch-all tests file for the crate.

## Const strings

When the same non-dynamic string literal is used more than once, prefer a
[compile-time constant](https://doc.rust-lang.org/std/keyword.const.html) when reasonable.
This reduces the risk of typos, especially for similar names such as event metadata fields.

## Code Organization

- `lib/`: shared libraries and isolated crates. Use these boundaries to enable reuse and keep
  dependency sets manageable.
- `src/`: Vector's application, components, configuration loading, and topology wiring.

## Internal telemetry: logging, metrics, traces

Emit component telemetry through internal events. Reuse existing events where possible; see the
[Instrumentation Specification](specs/instrumentation.md) for emission and naming requirements.
The examples below illustrate code inside event implementations.

### Logging

For logging, we use **[`tracing`](https://docs.rs/tracing/latest/tracing)**, which doubles as both a
way to emit logs but also a way to use distributed tracing techniques to add nested and contextual
metadata by utilizing [spans](https://docs.rs/tracing/latest/tracing/#spans).

#### Basic Usage

Use `tracing`'s event macros -- `trace!`, `debug!`, `info!`, `warn!`, and `error!` -- with
structured key/value fields. Put variable values in fields so they can be filtered and queried,
rather than interpolating them into the message.

#### Passing in the event message

Use the `message` field followed by the event's other fields:

```rust
info!(message = "Server has started.");
debug!(message = "User connected.", %username);
trace!(message = "Sent heartbeat packet to client.", bytes_sent = 22);
warn!(message = "Failed to merge value.", %error);
info!(message = "Processing batch.", batch_size, internal_log_rate_secs = 1);
```

Avoid putting error values into the message:

```rust
// Don't do this:
warn!("Failed to merge value: {}.", err);
// Do this instead:
warn!(message = "Failed to merge value.", %error);
```

Use `error` (not `e` or `err`) for error fields, and prefer Display formatting (`%error`) over
Debug formatting (`?error`).

#### Writing a good log message

- Write in English, capitalize sentences, and end them with a period.
- Keep messages short. Link to documentation when an explanation needs more than two sentences.
- Put variable values in structured fields; quote identifiers when they must appear in prose.

#### Choosing the right log level

- **TRACE**: Detailed diagnostics. Avoid unnecessary work in hot paths, even when logging is disabled.
- **DEBUG**: Troubleshooting information. Avoid logs proportional to event throughput unless rate limited.
- **INFO**: Normal lifecycle events, such as startup, reload, and shutdown.
- **WARN**: Unexpected conditions that operators should know about.
- **ERROR**: Failed operations and component errors, including failures that can be retried.
  Follow the instrumentation specification's error-event requirements; an error log does not
  necessarily mean data was lost or the process cannot recover.

### Metrics

Use Vector's typed `counter!`, `gauge!`, and `histogram!` wrappers, re-exported by `vector_lib`,
with `CounterName`, `GaugeName`, and `HistogramName`. Add new names to
[`metric_name.rs`](../lib/vector-common/src/internal_event/metric_name.rs).
Direct `metrics` macros with raw names are disallowed by [Clippy](../clippy.toml).

The wrappers return handles: update counters with `.increment()`, gauges with `.set()`,
`.increment()`, or `.decrement()`, and histograms with `.record()`.

```rust
use std::time::Duration;

use vector_lib::{
    counter, gauge, histogram,
    internal_event::{CounterName, GaugeName, HistogramName},
};

counter!(CounterName::HttpServerRequestsReceivedTotal).increment(1);
gauge!(GaugeName::OpenConnections).set(42.0);
gauge!(GaugeName::OpenConnections).increment(1.0);
gauge!(GaugeName::OpenConnections).decrement(1.0);
histogram!(HistogramName::HttpServerHandlerDurationSeconds, "status" => "200")
    .record(Duration::from_micros(750));
```

`metrics` converts `Duration` to seconds, so use a metric name with a seconds unit.
See the [metrics API](https://docs.rs/metrics/0.24.6/metrics/) and existing
[HTTP events](../src/internal_events/http.rs) for examples.

#### Choosing metric types

- **Counters** track cumulative totals, such as requests or bytes processed.
- **Gauges** track current values, such as open connections or queue size. Scrapes can miss
  short-lived peaks; cumulative enqueue/dequeue counters can expose activity between scrapes,
  but do not recover the peak queue size.
- **Histograms** track distributions, such as request latency or batch size. Their observation
  count is not necessarily an event count: one sample may represent a whole batch.

#### Best practices

- Bound label cardinality. Metric expiration removes idle series; it does not bound the number
  of active label combinations. Follow the component specification when particular labels are required.
- Batch metric updates where possible instead of emitting on every iteration of a tight loop.
- Reuse registered events and cached handles for frequent updates; see
  [`registered_event!`](../lib/vector-common/src/internal_event/mod.rs).
- Avoid redundant counters only when the histogram exposes the same quantity with the same labels
  through the relevant exporter. Preserve counters required by the component specification.

## Dependencies

### Error handling

Prefer [`snafu`](https://docs.rs/snafu) for structured errors with context and source errors.
Use `vector_lib::Error` (a boxed `std::error::Error + Send + Sync + 'static`) at boundaries that
need to accept different error types. Preserve useful context when propagating errors.

#### Panics

Code in Vector should **NOT** panic under normal circumstances.

- Panics are only acceptable when assumptions about internal state are violated (indicating a bug)
- All potential panics **MUST** be documented in function documentation
- Prefer `Result<T, E>` and proper error handling

### Concurrency and synchronization

Prefer `std::sync` locks for short critical sections that do not cross `.await`, including in
async code. Do not hold a synchronous lock across `.await` or block runtime threads on long work.
Use Tokio's async locks when a guard must span `.await`; see
[`tokio::sync::Mutex`](https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html).

#### Atomics

Prefer [standard-library atomics](https://doc.rust-lang.org/std/sync/atomic/index.html).
Choose memory ordering to match the synchronization requirements.

Use [`crossbeam_utils::atomic::AtomicCell`](https://docs.rs/crossbeam-utils/latest/crossbeam_utils/atomic/struct.AtomicCell.html)
when a suitable standard atomic type is unavailable. It can fall back to locks; use
`AtomicCell::is_lock_free()` when lock-free operation matters. Its loads use acquire ordering
and stores use release ordering; use standard atomics when explicit ordering control is required.

#### Global state

Use [`LazyLock`](https://doc.rust-lang.org/std/sync/struct.LazyLock.html) for values initialized
on first access, or [`OnceLock`](https://doc.rust-lang.org/std/sync/struct.OnceLock.html) when
initialization needs runtime inputs. Prefer these standard types over `once_cell` and `lazy_static`.

For shared snapshots with frequent reads and infrequent replacements, prefer
[`arc-swap`](https://docs.rs/arc-swap). It atomically replaces an `Arc<T>` while readers retain
access to their snapshots. Use `LazyLock` or `OnceLock` when storing it in a static.

#### Concurrent data structures

For concurrent indexed storage, prefer [`sharded-slab`](https://docs.rs/sharded-slab).
It reuses storage after entries are removed and also provides a pool for reusable objects.

## Feature Flags

### Component Feature Flags

New components (sources, sinks, transforms) must be behind feature flags:

```bash
# Build only specific component for faster iteration
make test FEATURES="sinks-console" SCOPE="sinks::console"
```

See the `features` section in [`Cargo.toml`](../Cargo.toml) for examples.

### Cargo Dependency Feature Placement

[Cargo features are additive](https://doc.rust-lang.org/cargo/reference/features.html#feature-unification).
Cargo combines features requested for the same dependency, subject to
[resolver boundaries](https://doc.rust-lang.org/cargo/reference/resolver.html#feature-resolver-version-2)
for build dependencies, targets, and dev dependencies. Declare the features each crate needs;
do not rely on another crate enabling them incidentally.

Always set `default-features = false` in `[workspace.dependencies]`. For feature declarations, the rule is simple:

- If only one crate needs a feature, declare it in that crate's own `Cargo.toml`.
- If multiple crates need it, declare it in `[workspace.dependencies]`.

Add a short comment near the dependency when the feature setup is non-obvious.

**Auditing:** when unsure whether a feature is truly needed or only transitively enabled, verify with:

```bash
# Show the full feature tree for the workspace
cargo tree --workspace -e features

# Narrow down to a specific dependency
cargo tree -e features -i <crate-name>
```

The `-i` flag shows which packages enable the dependency's features. Check the package and feature
selection you intend to build; a workspace-wide build can hide missing feature declarations.

## New Configuration Fields vs CLI flags

Use configuration fields for the data pipeline: sources, transforms, sinks, and persistent buffers.
Use CLI flags for process behavior such as shutdown timing.

For example, `vector --no-graceful-shutdown-limit` removes the timeout on graceful shutdown after
SIGINT or SIGTERM. Vector still starts shutting down and exits when draining finishes; a further
shutdown signal can force it to quit. See [`Opts`](../src/cli.rs) and the
[shutdown handler](../src/app.rs).
