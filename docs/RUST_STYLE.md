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

## Unit tests

Unit tests for a type live in the same source file as that type (`#[cfg(test)] mod tests` at
the bottom of the file) unless they exercise behavior that spans multiple modules. Cross-module
tests belong with the composing type, not in a catch-all tests file for the crate.

## Const strings

When the same non-dynamic string literal is used more than once, prefer a
[compile-time constant](https://doc.rust-lang.org/std/keyword.const.html) when reasonable.
This reduces the risk of typos, especially for similar names such as event metadata fields.

## Code Organization

Code is primarily split into two main directories: `lib/` and `src/`.

### `lib/`: shared libraries, etc

We use `lib` almost entirely for shared libraries and for isolating specific pieces of code. As
Vector itself involves a large number of dependencies, it can be beneficial to move code into
isolated crates under `lib/` in order to allow them not only to be shared, but also to reduce the
amount of code that must be processed by helper tools like `cargo check` and `rust-analyzer` during
normal development, which in turn speeds up the feedback loop between writing code and getting
informed about errors, warnings, and so on.

### `src/`: main binary and all related functionality

The bulk of functional code resides in the `src/` directory/crate.  When we refer to functional
code, we're talking about code that powers user-visible aspects of Vector, such as the sources,
transforms, and sinks themselves. There is also, of course, the requisite glue code such as parsing
command-line arguments, reading the configuration, constructing and configuring components, and
wiring them together.

## Internal telemetry: logging, metrics, traces

As a tool for ingesting, transforming, enriching, and shipping observability data, Vector has a
significant amount of its own internal telemetry. This telemetry is primarily logging and metrics,
but also includes some amount of tracing. Follow the
[Instrumentation Specification](specs/instrumentation.md) for event emission and naming
requirements; the examples below illustrate logging and metrics syntax.

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

In general, there are both a few rules and a few suggestions to follow when it comes to writing a
(good) log message:

- Messages must be written in English. No preference on which specific English dialect is used e.g.
  American English, British English, Canadian English, etc.
- Sentences must be capitalized, and end with a period.
- Proper spelling and grammar when possible. Not all of us are native English speakers, and so this
  is simply an ask, but not a hard requirement.
- Identifiers, or passages of note, should be called out by some means i.e. wrapping them in
  backticks or quotes.  Wrapping with special characters can be helpful in drawing the users eye to
  anything of importance.
- If it's longer than one or two sentences, it's probably better suited as a single sentence briefly
  explaining the event, with a link to external documentation that explains further.

#### Choosing the right log level

Similarly, choosing the right level can be important, both from the perspective of making it easy
for users to grok what they should pay attention to, but also to avoid the performance overhead of
excess logging (even if we filter it out and it never makes it to the console).

- **TRACE**: Typically contains a high level of detail for deep/rich debugging.

  As trace logging is typically reached for when instrumenting algorithms and core pieces of logic,
  care should be taken to avoid trace logging being added to tight loops, or commonly used
  codepaths, where possible. Even when disabled, there can still be a small overhead associated with
  logging an event at all.
- **DEBUG**: Basic information that can be helpful for initially debugging issues.

  Should typically not be used for things that happen per-event, or scales with event throughput,
  but in some cases -- i.e. if it happens every 1000th event, etc -- it can safely be used.
- **INFO**: Common information about normal processes.

  This includes logical/temporal events such as notifications when components are stopped and
  started, or other high-level events that, crucially, do not represent an event that an operator
  needs to worry about.

  Said another way, **INFO** is primarily there for information that lets them know that an action
  they just took completed successfully, whether that's the server initially starting up
  successfully, or reloading a configuration successfully, or exiting Vector after receiving
  SIGTERM.
- **WARN**: Something unexpected happened, but no data has been lost, nothing has crashed, and we
  can recover from it without an issue. An operator might be interested in something at the **WARN**
  level, but it shouldn't be informing them of things serious enough to require immediate attention.
- **ERROR**: Data loss, unrecoverable errors, and anything else that will require an operator to
  intervene and recover from. These should be rare so that they maintain a high signal-to-noise
  ratio in the observability tooling that operators themselves are using.

### Metrics

For metrics, we use **[`metrics`](https://docs.rs/metrics/latest/metrics/)**, which like `tracing`,
provides macros for emitting counters, gauges, and histograms.

#### Basic Usage

There are three basic metric types: counters, gauges, and histograms. **Counters** are meant for
counting things, such as the total number of requests processed, where the count only grows over
time. This is also sometimes called a *monotonic counter*, or a *monotonically increasing* counter.
**Gauges** are for tracking a single value that changes over time, and can go up and down, such as
the number of current connections. **Histograms** are for tracking multiple observations of the same
logical event, such as the time it takes to serve a request.

Emitting metrics always involves a metric name and a value being measured. They can optionally
accept descriptive labels:

```rust
// Counters can be incremented by an arbitrary amount, or `increment_counter!`
// can be used to simply increment by one:
counter!("bytes_sent_total", 212);
increment_counter!("requests_processed_total", "service" => "admin_grpc");

// Gauges can be set to an absolute value, such as setting it to the latest value of an
// external measurement, or it can be incremented and decremented by an arbitrary amount:
gauge!("bytes_allocated", 42.0);
increment_gauge!("bytes_allocated", 2048.0, "table_name" => self.table_name.to_string());
increment_gauge!("bytes_allocated", 512.0, "table_name" => self.table_name.to_string());
decrement_gauge!("bytes_allocated", 2560.0, "table_name" => self.table_name.to_string())

// Histograms simply record a measurement, but there's a fun little trait that `metrics`
// uses called `IntoF64` that lets custom types provide a way to convert themselves to a
// `f64`, which there's a default implementation of for `Duration`:
let delta = Duration::from_micros(750);
histogram!("request_duration_ns", delta);
histogram!("request_duration_ns", 742_130, "endpoint" => "frontend");
```

#### Avoiding pitfalls with gauges

Many values can appear, at first, to be best tracked as a gauge: current connection count, in-flight
request count, and so on. However, in some cases, the value being measured may change too frequently
to be reliably tracked.  Metrics are typically collected on an interval, which is fine for counters
and histograms: they're purely additive.  However, since a gauge is simply the _latest_ value, you
cannot know _how_ it's changed since the last time you've observed it.

This is a common problem where a gauge tracks something like a queue size. If there's an event where
the queue grows rapidly but drains back down quickly, you may not ever observe the gauge having
changed if your collection interval is greater than the duration of such events.

A simple pattern to follow to handle these scenarios is to use two counters -- one for the
increments, one for the decrements -- so that you can graph the difference between them, giving you
the equivalent of the "current" value. In our example above, we might have `queue_items_pushed` and
`queued_items_popped`, and if `queue_items_pushed` equals 100, and `queued_items_popped` equals 80,
we know our queue size is 20. More importantly, if we queried both of them at the same time, and
they were both zero, and then queried them both a second later, and saw that both were 100,000, we
would know that the queue size was _currently_ zero but we'd also know that we just processed
100,000 items in the past second.

#### Best Practices

- **Do** attempt to limit the cardinality, or number of unique values, of label values. If the
  number of unique values for a label grows over time, this can represent a large amount of consumed
  memory. This is a problem we expect to be fixed in the medium-term
  ([#11995](https://github.com/vectordotdev/vector/issues/11995)) but is a good rule to follow
  unless there's a competing reason to do so, such as when following the guidelines in the
  [Component
  Specification](https://github.com/vectordotdev/vector/blob/master/docs/specs/component.md).
- **Don't** emit metrics in tight loops. Each metric emission carries an overhead, and emitting them
  in tight loops can cause that overhead to become noticeable in terms of CPU usage and throughput
  reduction. Instead of incrementing a counter every time a loop iteration occurs, you might
  consider incrementing a local variable instead, and then emitting that sum after the loop is over.
- **Don't** update a counter to measure the total number of operations/events/etc if you're already
  tracking a histogram of those operations/events/etc. Histograms have a `count` property that
  counts how many samples the histogram has recorded, as well as a `sum` property that is a sum of
  the value of all samples the histogram has recorded. This means you can potentially get three
  metrics for the cost of emitting one.

## Dependencies

### Error handling

For error handling, there are two parts: _creating errors_ and _working with errors_.

For **creating errors**, we prefer **[`snafu`](https://docs.rs/snafu)**. The `snafu` crate provides
a derive for generating the boilerplate `std::error::Error` implementation on your custom error
struct or enum. It additionally provides helpers for defining the `Display` output for your error
(potentially on a per-variant basis when working with enums).

While there are popular alternatives such as [`failure`](https://docs.rs/failure) and
[`thiserror`](https://docs.rs/thiserror), they generally lack either the thoroughness in
documentation or the flexibility of `snafu`.

For **working with errors**, we have a more lax approach. At the highest level, we use a boxed trait
approach -- `Box<dyn std::error::Error + Send + Sync + 'static>` -- for maximum flexibility. This
allows developers to avoid needing to _always_ derive custom error types in order to return errors
back up the call stack. This does not prevent, and indeed, should not discourage developers from
using `snafu` to create rich error types that provide additional context, whether through
descriptive error messages, source errors, or backtraces.

#### Panics

Code in Vector should **NOT** panic under normal circumstances.

- Panics are only acceptable when assumptions about internal state are violated (indicating a bug)
- All potential panics **MUST** be documented in function documentation
- Prefer `Result<T, E>` and proper error handling

### Concurrency and synchronization

Prefer `std::sync` primitives such as `Mutex` and `RwLock` for synchronous code.

In async code, synchronous locks can block runtime threads and cause deadlocks. Use Tokio's
async primitives when needed; see the
[`tokio::sync::Mutex` documentation](https://docs.rs/tokio/latest/tokio/sync/struct.Mutex.html)
for guidance on choosing between synchronous and async mutexes.

#### Atomics

In general, we strive to use the atomic types in the [standard
library](https://doc.rust-lang.org/stable/std/sync/atomic/index.html) when possible, as they are the
most portable and well-tested. In cases where the standard library atomics cannot be used, such as
when using a 64-bit atomic but wanting to support a 32-bit platform, or support a platform without
atomic instructions at all, we prefer to use
**[`crossbeam-utils`](https://docs.rs/crossbeam-utils)** and its `AtomicCell` helper. This type will
automatically handle either using native atomic support or ensuring mutually exclusive access, and
handle it in a transparent way. It uses a fixed acquire/release ordering that generally provides the
expected behavior when using atomics, but may not be suitable for usages which require stronger
ordering.

#### Global state

When there is a need or desire to share global state, there are a few options depending on the
required constraints.

If you're working with data that _never changes after initialization_,
we prefer `std::sync::OnceLock` over **[`once_cell`](https://docs.rs/once_cell)** or
[`lazy_static`](https://docs.rs/lazy-static). It is slightly faster and provides a richer API than
`lazy_static`, and has equivalent features to the `once_cell` version.

If you're working with data that _changes over time_, but has a very high read-to-write ratio, such
as _many readers_, but _one writer_ and infrequent writes, we prefer
**[`arc-swap`](https://docs.rs/arc-swap)**.  The main feature of this crate is allowing a piece of
data to be atomically updated while being shared concurrently. It does this by wrapping all data in
`Arc<T>` to provide the safe, concurrent access, while adding the ability to atomically swap the
`Arc<T>` itself. As it cannot be constructed in a const fashion, `arc-swap` pairs well with
`std::sync::OnceLock` for actually storing it in a global static variable.

#### Concurrent data structures

When there is a need for a concurrent and _indexable_ storage, we prefer
**[`sharded-slab`](https://docs.rs/sharded-slab)**.  This crate provides a means to insert items
such that the caller gets back to the index by which it can access the item again in the future.
Additionally, when an item is removed, its storage can be reused by future inserts, making
`sharded-slab` a good choice for long-running processes where memory allocation reduction is
paramount. There is also a pool data structure based on the same underlying design of the slab
itself for use cases where pooling is desired.

## Feature Flags

### Component Feature Flags

New components (sources, sinks, transforms) must be behind feature flags:

```bash
# Build only specific component for faster iteration
cargo test --lib --no-default-features --features sinks-console sinks::console
```

See the `features` section in [`Cargo.toml`](../Cargo.toml) for examples.

### Cargo Dependency Feature Placement

[Cargo features are additive](https://doc.rust-lang.org/cargo/reference/features.html#feature-unification): once any crate in the dependency graph enables a feature, it is enabled for the entire build. This can be surprising — enabling a feature in one crate silently turns it on everywhere.

Always set `default-features = false` in `[workspace.dependencies]`. For feature declarations, the rule is simple:

- If only one crate needs a feature, declare it in that crate's own `Cargo.toml`.
- If multiple crates need it, declare it in `[workspace.dependencies]`.

Add a short comment near the dependency when the feature setup is non-obvious.

**Auditing:** when unsure whether a feature is truly needed or only transitively enabled, verify with:

```bash
# Show the full feature tree for the workspace
cargo tree -e features

# Narrow down to a specific dependency
cargo tree -e features -i <crate-name>
```

The `-i` flag shows which crates depend on `<crate-name>` and which features they activate, useful for tracing where a feature is coming from. If a feature only appears because another crate enables it, and your crate relies on that feature being present, declare it explicitly — otherwise your crate silently breaks if the other crate ever stops enabling it.

## New Configuration Fields vs CLI flags

Vector makes the distinction between configuration items that are essential to understand data
pipelines and runtime flags that determine the details of the runtime behavior. The main configuration
generally lives in a file in the current directory or in `/etc/vector`.

Examples of main configuration fields are source, transformation, and sink declaration, as well as
information about where any disk buffers should be persisted.

For configuration items that purely inform details of Vector's runtime behavior, CLI flags without
corresponding configuration fields should be used.

An example of a runtime flag is
`vector run --no-graceful-shutdown-limit`, which tells Vector to ignore SIGINTs and to continue running
as normal until a SIGKILL is received. In this case, as the configuration describes the desired runtime
behavior in a specific environment and not to the underlying data pipeline, no corresponding field in
the configuration file should exist.
