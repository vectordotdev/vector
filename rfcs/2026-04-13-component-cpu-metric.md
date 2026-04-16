# RFC 2026-04-13 - Per-component CPU time metric for sync transforms

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
- The `enable_concurrency` trait method controls whether a transform is
  dispatched to parallel `tokio::spawn` tasks (up to
  `TRANSFORM_CONCURRENCY_LIMIT`, which defaults to the number of worker
  threads).

## Cross cutting concerns

- The `utilization` gauge remains as-is. This RFC adds a complementary metric;
  it does not replace the existing one.
- Future work could extend this approach to task transforms and sinks.

## Scope

### In scope

- A new `component_cpu_usage_ns_total` counter for **sync and function
  transforms** (both inline and concurrent execution paths).
- Two implementation tiers: a wall-clock fallback that works everywhere, and a
  precise thread-CPU-time implementation using OS APIs.
- Feasibility analysis of thread-level CPU time measurement.

### Out of scope

- Task transforms (async stream-based). Their execution interleaves with the
  tokio runtime in ways that make per-poll CPU measurement a distinct problem.
  Furthermore, all task transforms in Vector are currently single-threaded (they
  do not parallelize work), making the `utilization` metric a good indicator of
  their actual usage.
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

A new counter metric is emitted for every sync/function transform:

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

This metric is always emitted for sync/function transforms; there is no
configuration knob.

### Implementation

#### Metric definition

Register a `counter!("component_cpu_usage_ns_total")` in the `Runner` struct,
alongside the existing `timer_tx` and `events_received` fields:

```rust
struct Runner {
    transform: Box<dyn SyncTransform>,
    input_rx: Option<BufferReceiver<EventArray>>,
    input_type: DataType,
    outputs: TransformOutputs,
    timer_tx: UtilizationComponentSender,
    latency_recorder: LatencyRecorder,
    events_received: Registered<EventsReceived>,
    cpu_ns: Counter,  // NEW
}
```

Using nanoseconds as the counter unit fits naturally in the `metrics` crate's
`u64`-based `Counter`: each call to `counter.increment(delta.as_nanos() as u64)`
is exact integer arithmetic with no floating-point accumulation. The `metrics`
crate stores the counter as `AtomicU64` and casts to `f64` only once at scrape
time, bounding the conversion error to a single rounding rather than an
accumulated sum of many small imprecise additions.

This choice was necessary because the `tracing` API only supports integer
increments of counters, unlike `host_cpu_seconds_total` which is directly
submitted as a floating number.

#### Supported OS: Thread CPU time (precise, Linux and macOS)

For precise measurement, we read the calling thread's CPU clock before and after
`transform_all`. This counts only time the thread was actually scheduled on a
CPU, excluding preemption, involuntary context switches, and any time another
process used the core.

**Linux and macOS — `clock_gettime(CLOCK_THREAD_CPUTIME_ID)`**

```rust
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn thread_cpu_time() -> Duration {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: ts is a valid pointer to a timespec struct and
    // CLOCK_THREAD_CPUTIME_ID is a valid clock id on Linux >= 2.6
    // and macOS >= 10.12.
    unsafe {
        libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts);
    }
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}
```

This API is available on both Linux and macOS (since 10.12 Sierra, 2016). It
measures CPU time for the **calling thread** with nanosecond granularity.
Since `transform_all` is synchronous and runs entirely within a single thread
poll, the delta between two calls around `transform_all` gives exact CPU time
consumed by that transform invocation.

**Overhead:** On Linux, `clock_gettime(CLOCK_THREAD_CPUTIME_ID)` is
vDSO-accelerated and costs ~20-60ns per call. On macOS the kernel path is
slightly heavier (~100-200ns) but still negligible compared to actual transform
work. With two calls per `transform_all` invocation the total overhead is
well under 500ns per batch on either platform.

**Windows — `GetThreadTimes`**

Windows exposes per-thread CPU time via `GetThreadTimes`, providing the same
guarantee as `CLOCK_THREAD_CPUTIME_ID` with 100ns granularity. It is
implemented using the `windows-sys` crate (added as a
`[target.'cfg(windows)'.dependencies]`), which is already a transitive
dependency of Vector.

```rust
#[cfg(target_os = "windows")]
fn thread_cpu_time() -> Duration {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};

    let mut creation = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut exit     = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut kernel   = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let mut user     = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };

    // SAFETY: GetCurrentThread() returns a pseudo-handle that is always valid.
    unsafe {
        GetThreadTimes(GetCurrentThread(), &mut creation, &mut exit, &mut kernel, &mut user);
    }

    // FILETIME stores time as 100ns intervals split across two u32 fields.
    let to_nanos = |ft: FILETIME| {
        (((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64) * 100
    };
    Duration::from_nanos(to_nanos(kernel) + to_nanos(user))
}
```

#### Fallback: Wall-clock timing (other platforms)

On any platform not covered above, fall back to wall-clock time:

```rust
fn thread_cpu_time() -> Duration {
  Instant::now()
}
```

This is simple and portable. Its accuracy is good for CPU-bound sync transforms
because `transform_all` is a synchronous call that does not yield to the tokio
runtime. The main source of inaccuracy is OS-level thread preemption: if the OS
schedules another process onto the core during `transform_all`, that wall-clock
time is counted as CPU time even though Vector was not executing.

For small workloads (lightly loaded hosts, transforms that complete in
microseconds to low nanoseconds), the preemption error is negligible.

#### Why thread CPU time works for sync transforms

The critical property that makes this approach accurate is that `transform_all`
is a **synchronous, non-yielding** call. When the tokio runtime polls the future
containing `transform_all`:

1. The runtime's worker thread enters the poll.
2. `transform_all` executes to completion without any `.await` points.
3. Control returns to the runtime.

Between steps 1 and 3, the worker thread is exclusively executing transform
code. Reading the thread CPU clock before and after captures exactly the CPU
time consumed, regardless of:

- Other tokio tasks that may be queued (they can't preempt a synchronous call).
- OS preemption (thread CPU clock excludes time spent not running).
- Other concurrent `tokio::spawn` tasks on different threads (each measures its
  own thread independently).

This would **not** work for async task transforms, where a single `poll` may
interleave with unrelated futures on the same worker thread.

#### Concurrent execution and multi-core accounting

In the concurrent path (`run_concurrently`), each `tokio::spawn` task measures
its own CPU time independently and increments the shared counter handle (which
is `Clone` and backed by `AtomicU64::fetch_add`). If 4 tasks each consume 250ms
of CPU in parallel, the counter increments by 1000ns total — correctly
reflecting that the transform used 1 CPU-second even though only 250ms of wall
time elapsed.

#### Integration into the Runner

```rust
impl Runner {
    async fn run_inline(mut self) -> TaskResult {
        const INLINE_BATCH_SIZE: usize = 128;
        let mut outputs_buf = self.outputs.new_buf_with_capacity(INLINE_BATCH_SIZE);
        let mut input_rx = self.input_rx.take().expect("can't run runner twice")
            .into_stream()
            .filter(move |events| ready(filter_events_type(events, self.input_type)));

        self.timer_tx.try_send_start_wait();
        while let Some(events) = input_rx.next().await {
            self.on_events_received(&events);

            let t0 = ThreadTime::now();
            self.transform.transform_all(events, &mut outputs_buf);
            self.cpu_ns.increment(t0.elapsed().as_nanos() as u64); // NEW

            self.send_outputs(&mut outputs_buf).await
                .map_err(TaskError::wrapped)?;
        }
        Ok(TaskOutput::Transform)
    }

    async fn run_concurrently(mut self) -> TaskResult {
        // ... (existing setup) ...
        input_arrays = input_rx.next(), ... => {
            match input_arrays {
                Some(input_arrays) => {
                    // ... (existing event counting) ...
                    let mut t = self.transform.clone();
                    let mut outputs_buf = self.outputs.new_buf_with_capacity(len);
                    let cpu_ns = self.cpu_ns.clone(); // NEW
                    let task = tokio::spawn(async move {
                        let t0 = ThreadTime::now();
                        for events in input_arrays {
                            t.transform_all(events, &mut outputs_buf);
                        }
                        cpu_ns.increment(t0.elapsed().as_nanos() as u64); // NEW
                        outputs_buf
                    }.in_current_span());
                    in_flight.push_back(task);
                }
                // ...
            }
        }
        // ...
    }
}
```

#### Module structure

Add a new module `src/cpu_time.rs`:

```rust
/// Returns the CPU time consumed by the calling thread.
///
/// On Linux and macOS, uses clock_gettime(CLOCK_THREAD_CPUTIME_ID) (nanosecond precision).
/// On other platforms, falls back to Instant::now() (wall-clock time).
pub(crate) fn thread_cpu_time() -> Duration { ... }
```

This keeps the platform-specific FFI contained in one file and testable
independently.

## Rationale

- **Direct CPU cost visibility.** Operators can identify which transforms are
  CPU-bottlenecked vs. backpressure-limited, enabling informed tuning.
- **Composable with existing metrics.** `rate(component_cpu_usage_ns_total[1m]) / 1e9`
  gives CPU cores used; dividing by `utilization` separates CPU from pipeline effects.
- **Low overhead.** Two `clock_gettime` calls per batch (~80ns total on Linux)
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

- Add `src/cpu_time.rs` module with `thread_cpu_time()` and platform-specific
  implementations behind `#[cfg]` gates. Include unit tests that verify the
  returned duration is non-zero and monotonically increasing.
- Register `counter!("component_cpu_usage_ns_total")` in `Runner::new` and
  instrument `run_inline` with wall-clock timing (Tier 1).
- Instrument `run_concurrently` with wall-clock timing (Tier 1). Verify the
  counter increments correctly when multiple tasks run in parallel.
- Switch from `Instant::now()` to `thread_cpu_time()` (Tier 2). Benchmark
  the overhead on Linux to confirm it is <100ns per call.
- Add integration test: run a CPU-intensive remap transform, verify
  `component_cpu_usage_ns_total` is within 10% of expected CPU time.
- Add documentation for the new metric in the generated component docs.
- Add changelog fragment.

## Future Improvements

- Extend `component_cpu_usage_ns_total` to **task transforms** by measuring CPU
  time per `poll` of the transform stream. This requires careful accounting to
  exclude time spent in the tokio runtime between polls.
- Extend to **sources and sinks** where the component owns a synchronous
  processing step (e.g., codec encoding in sinks).
- Expose a derived `**cpu_utilization` gauge\*\* (CPU seconds / wall seconds)
  computed by the `UtilizationEmitter` for operators who prefer a ready-to-use
  ratio.
- Add `mode="user"` / `mode="system"` tag split for deeper CPU profiling.
