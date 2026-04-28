# RFC 2026-04-13 - Per-component CPU time metric for transforms

The current `utilization` gauge measures the fraction of wall-clock time a
component is not idle (i.e., not waiting on its input channel). Because sync
and function transforms can run concurrently across multiple tokio worker
threads, and because wall-clock "not idle" includes time the OS has preempted
the thread, this gauge does not accurately reflect how much CPU a component
actually consumes. This RFC proposes a new **counter** metric,
`component_cpu_usage_ns_total`, that tracks the cumulative CPU time consumed
by a component's transform work in nanoseconds, measured via OS thread-level
CPU clocks.

## Context

- The existing `utilization` metric is implemented in `src/utilization.rs`.
- Sync and function transforms are spawned in `src/topology/builder.rs`
  via the `Runner` struct (`run_inline` and `run_concurrently` methods).
- Task transforms are built in `src/topology/builder.rs::build_task_transform`
  and run as a single async task driving a stream-based pipeline.
- The `enable_concurrency` trait method controls whether a sync transform is
  dispatched to parallel `tokio::spawn` tasks (up to
  `TRANSFORM_CONCURRENCY_LIMIT`, which defaults to the number of worker
  threads).

## Cross cutting concerns

- The `utilization` gauge remains as-is. This RFC adds a complementary metric;
  it does not replace the existing one.
- Future work could extend this approach to sources and sinks.

## Scope

### In scope

- A new `component_cpu_usage_ns_total` counter for **all transforms** —
  sync and function transforms (both inline and concurrent execution paths)
  and task transforms.
- Two implementation tiers: a wall-clock fallback that works everywhere, and a
  precise thread-CPU-time implementation using OS APIs.
- Feasibility analysis of thread-level CPU time measurement.

### Out of scope

- Sources and sinks.
- Replacing or modifying the existing `utilization` gauge.

## Pain

1. **Utilization is misleading under concurrency.** In the concurrent
   `run_concurrently` path, the utilization timer stays in "not waiting" state
   from the moment events are received (`stop_wait` in `on_events_received`)
   until a completed task's output is sent (`start_wait` in `send_outputs`).
   The actual CPU work happens on separate `tokio::spawn`'d tasks that the
   timer does not track. This means utilization measures **occupancy** (is
   there at least one batch in flight?) rather than CPU consumption.

   Concrete example: a concurrent remap with 4 in-flight tasks each taking
   10ms, input arriving every 5ms. Input arrives frequently enough that
   `stop_wait` fires before each spawn, keeping the timer in "not waiting"
   almost continuously → utilization ≈ 100%. But actual CPU consumption is
   4 × 10ms / 20ms = 2 cores. The utilization gauge cannot distinguish
   "2 cores" from "0.3 cores at 100% occupancy."

2. **No way to detect CPU-bound transforms.** Operators tuning pipelines need to
   know which transforms are CPU-bottlenecked. A `cpu_usage_ns_total` counter,
   when divided by wall-clock time (in ns), directly gives CPU core utilization
   and can exceed 1.0 when a transform genuinely uses multiple cores.

## Proposal

### User Experience

A new counter metric is emitted for every transform:

```prometheus
component_cpu_usage_ns_total{component_id="my_remap",component_kind="transform",component_type="remap"} 14207
```

The value is cumulative CPU nanoseconds consumed by the component. Operators
use it to compute CPU core utilization:

```promql
# Per-component CPU core usage (can exceed 1.0 with concurrency)
rate(component_cpu_usage_ns_total{component_id="my_remap"}[1m]) / 1e9

# Compare against utilization to separate CPU cost from pipeline pressure
rate(component_cpu_usage_ns_total{component_id="my_remap"}[1m]) / 1e9
  /
  utilization{component_id="my_remap"}
```

This metric is always emitted for transforms; there is no configuration knob.

## Rationale

- **Direct CPU cost visibility.** Operators can identify which transforms are
  CPU-bottlenecked vs. backpressure-limited, enabling informed tuning.
- **Composable with existing metrics.** `rate(component_cpu_usage_ns_total[1m]) / 1e9`
  gives CPU cores used; dividing by `utilization` separates CPU from pipeline effects.
- **Measurement is hooked at the task's poll boundary.** For the concurrent
  sync path and for task transforms, the spawned tokio task's future is wrapped
  in an adapter that samples thread CPU time around every call to
  `Future::poll`. Tokio's cooperative scheduler guarantees that within a single
  poll the task cannot be moved to another worker thread and no other task can
  run on the current thread, so each `(before_poll, after_poll)` pair is a
  clean per-thread measurement. Multiple polls (across `Pending` returns and
  wake-ups) accumulate correctly, with each poll independently sampling the
  thread it ran on. This isolates the timing concern from the transform body
  and keeps it robust if the body ever grows `.await` points (which task
  transforms have many of, by construction).
- **Scope of the measurement, and isolation from upstream components.**
  Vector components communicate only through `BufferReceiver` / `BufferSender`
  channels — never through stream combinators chained across component
  boundaries. Each component runs in its own tokio task with its own poll
  cycles. So when our task polls its input, it dequeues items from a shared
  channel buffer; it does **not** run the upstream component's code. The
  upstream produced those items earlier, in its own task, and its CPU was
  already charged to its own `cpu_ns` counter at that point. This holds even
  in the "channel is always full when we poll" case: those items were produced
  by upstream CPU that was already counted upstream; we're just dequeueing them.

  The transform's `cpu_ns` therefore **includes**:

  - **Sync transforms** (inline and concurrent): exactly the synchronous
    `transform_all` call.
  - **Task transforms**: the entire task body — input-channel dequeue,
    `Utilization` / `OutputUtilization` stream-wrapper overhead, the
    user-defined transform's `poll_next`, the schema/latency `map`, and the
    fanout-send loop. A single poll may churn through many items before
    tokio's cooperative budget (default 128 ops) forces a `Pending`; all of
    that is genuinely this task's work.
  - **Helper tokio tasks** the transform spawns at construction time
    (`aws_ec2_metadata`'s IMDS-refresh worker, `throttle`'s
    `RateLimiterRunner` flush loop): the `cpu_ns` counter is plumbed through
    `TransformContext` so those helpers wrap their spawn with the same
    `.cpu_timed(counter)`. Their CPU is attributed back to this component
    rather than silently excluded.

  And **does not** include:

  - The upstream component's transform/source CPU (stays on upstream's
    counter, runs in upstream's task).
  - Time our task was parked in `Pending` waiting for input (no polls happen,
    so no CPU ticks).
  - Other tokio tasks running on other worker threads.

  The channel-poll / fanout-send bookkeeping our wrapper does include is
  small relative to the transform's own work, so the metric remains a
  meaningful comparator across transform kinds.
- **Low overhead.** Two `clock_gettime` calls per poll (~80ns total on Linux)
  is negligible relative to the work `transform_all` performs.
- **No accumulation errors.** The counter stores `u64` nanoseconds; each
  increment is exact integer arithmetic. The single `u64 → f64` cast at scrape
  time has bounded, non-accumulated error.

## Drawbacks

- **Platform-specific code.** The precise implementation uses `cfg`-gated FFI
  for Linux, macOS, and Windows. Other platforms fall back to wall-clock time,
  giving three maintained code paths plus one fallback.

## Alternatives

### Extend the existing utilization gauge

Add a CPU-time-based "utilization v2" that replaces the current gauge.

**Rejected because:** The current utilization metric serves a different purpose
(pipeline flow analysis: is this component starved or saturated?). CPU time is a
complementary signal, not a replacement. Conflating them would lose information.

### Per-event latency histogram

Emit a histogram of per-event processing time instead of a cumulative counter.

**Rejected because:** Histograms are expensive at high throughput (Vector
processes millions of events/sec). A counter that increments once per batch is
far cheaper. Per-event latency can be derived from the counter and
`events_sent_total` if needed (`cpu_ns / events = avg cpu ns per event`).

### `getrusage(RUSAGE_THREAD)` instead of `clock_gettime`

On Linux, `getrusage(RUSAGE_THREAD)` also provides per-thread CPU time (as
`ru_utime` + `ru_stime`).

**Not preferred because:** `clock_gettime(CLOCK_THREAD_CPUTIME_ID)` has
nanosecond precision vs. microsecond for `getrusage`. Both are vDSO-accelerated
on modern kernels. The higher precision is worth the identical cost.

## Outstanding Questions

1. **User/system split:** Should we report user and system CPU time separately
   (as `mode="user"` / `mode="system"` tags) like `host_cpu_seconds_total`
   does? The Linux API supports this. It adds cardinality but helps distinguish
   transforms that trigger syscalls (e.g., enrichment table lookups) from pure
   computation.

## Plan Of Attack

- Add `src/cpu_time.rs` module exposing:
  - A `ThreadTime` snapshot with platform-specific implementations behind
    `#[cfg]` gates (Linux/macOS `CLOCK_THREAD_CPUTIME_ID`, Windows
    `GetThreadTimes`, wall-clock fallback elsewhere). Include unit tests that
    verify the returned duration is non-negative and monotone.
  - A `CpuTimedFuture<F>` adapter that wraps a future and, on every
    `Future::poll`, samples `ThreadTime` before and after the inner poll and
    increments a `metrics::Counter` by the delta. A `CpuTimedExt` extension
    trait exposes it as a chained `.cpu_timed(counter)` call, mirroring
    `tracing::Instrument::in_current_span`.
- Add a `cpu_ns: Counter` field to `TransformContext`, defaulting to
  `Counter::noop()`. In `build_transforms`, register the counter inside the
  transform `error_span!` so it is tagged with `component_id`,
  `component_kind`, and `component_type`, then store it on the context. This
  is the single resolved handle every transform path consumes — sync, task,
  and any helper tokio tasks — so labels and recorder lookup are paid once,
  not on every poll. Also propagate the same handle through `TransformNode`
  for the topology builder's own use.
- For `run_inline`, bracket the synchronous `transform_all` call directly with
  `ThreadTime::now()` / `elapsed()`. The transform task itself owns this code
  and there is no `.await` between the brackets, so inline measurement is the
  simplest correct option.
- For `run_concurrently`, wrap the spawned per-batch future via
  `.cpu_timed(cpu_ns.clone())` rather than measuring inline. This hooks the
  measurement onto the task's `Future::poll` boundary and makes the pattern
  uniform for any future async work added inside the spawned body.
- For `build_task_transform`, take the counter from `TransformNode` (which
  receives it from the context) and wrap the outer task future with
  `.cpu_timed(counter)` before `.boxed()`. CPU time accumulates across every
  poll of the task, naturally excluding time the task is parked in `Pending`.
- For transforms that spawn helper tokio tasks at construction time
  (`aws_ec2_metadata`, `throttle`'s `RateLimiterRunner`), read
  `context.cpu_ns.clone()` in `build()` and `.cpu_timed(...)` the spawned
  helper future. For `RateLimiterRunner::start`, plumb the counter through as
  a parameter so it stays the throttle config's responsibility to provide it.
- Add integration test: run a CPU-intensive remap transform, verify
  `component_cpu_usage_ns_total` is within 10% of expected CPU time.
- Add documentation for the new metric in the generated component docs.
- Add changelog fragment.

## Future Improvements

- Extend to **sources and sinks** where the component owns a synchronous
  processing step (e.g., codec encoding in sinks).
- Expose a derived **`cpu_utilization` gauge** (CPU seconds / wall seconds)
  computed by the `UtilizationEmitter` for operators who prefer a ready-to-use
  ratio.
- Add `mode="user"` / `mode="system"` tag split for deeper CPU profiling.
