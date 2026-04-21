# GCL Sink Stall Investigation

## Background

Etsy runs Vector on GKE to route logs from Kafka topics to Google Cloud Logging (GCL). Each pod
runs a single Vector process with a GCL sink driven by the standard `Driver` loop in
`lib/vector-stream/src/driver.rs`.

On 2026-04-21, pod `vector-9c594f86-sw47n` stopped delivering logs to GCL at approximately
**18:28Z**. The pod was eventually killed by Kubernetes and replaced. The goal of this
investigation is to determine why the GCL sink's driver loop became permanently stuck.

---

## Observed Symptoms

| Time (UTC) | Observation |
|---|---|
| 18:24–18:27 | Stall-check warnings fire for the GCL sink (`in_flight_requests > 0`). `stalled_secs=0` on each warning — requests were still completing. False alarms. |
| 18:27:19 | Last GCL sink stall warning with `in_flight > 0`. |
| 18:28:19 | First stall-check tick after which **no further GCL sink warnings appear** — driver has silently transitioned to a state where the stall check produces no output at all. |
| 18:28–18:30 | Back-pressure propagates: Kafka source stops consuming (consequence, not cause). |
| ~18:30 | `prom_exporter` sink continues emitting metrics normally throughout — the Vector process is alive. |
| SIGTERM | Driver dump shows `in_flight=0`, `next_batch=None`. Driver is suspended in **arm 3** of the `select!` (`batched_input.next()`). |

The critical window is between **18:27:19 and 18:28:19**. In that 60-second interval the driver
transitioned from "requests completing normally" to "permanently stuck awaiting the next batch from
the input stream."

---

## Driver Architecture

The driver's `select!` loop has four arms, evaluated in priority order (`biased`):

```
Arm 1: in_flight.next()            — completes a finished service call
Arm 2: poll_fn(service.poll_ready) — fires when service is ready AND next_batch is Some
Arm 3: batched_input.next()        — fires when a new batch arrives AND next_batch is None
Arm 4: stall_check.tick()          — fires every 60 seconds
```

At SIGTERM the driver was in the state:

- `in_flight = 0` → arm 1 guard is false, arm 1 never fires
- `next_batch = None` → arm 2 guard is false, arm 2 never fires
- arm 3 is active but `.await` on `batched_input.next()` returned `Poll::Pending` and never woke

Before this investigation, arm 4 only logged when `in_flight > 0` or `next_batch.is_some()`. With
`in_flight=0` and `next_batch=None`, arm 4 fired every 60s but produced **no log output** — the
silent gap that made this stall invisible.

---

## The Input Stream Chain

`batched_input` is a layered async stream:

```
BoxStream<Event>               (kafka source output)
  └─ Batcher                   (accumulates events into timed/sized batches)
       └─ ConcurrentMap        (builds service Requests, wraps request_builder)
            └─ ready_chunks    (groups into chunks up to 1024)
                 └─ batched_input (what arm 3 polls)
```

For `batched_input.next()` to return `Poll::Pending` and never wake, **one of these layers must
have lost its waker or stopped making forward progress**.

---

## What We Know

### The stall is pipeline-specific, not process-wide

`prom_exporter` (a separate sink in the same process) continued emitting metrics throughout the
stall. The Tokio runtime and all other pipeline components remained healthy. The stall is isolated
to the GCL sink's driver and its upstream stream chain.

### The stall-check silence was a known blind spot (now fixed)

Prior to our changes, arm 4 only produced output in two cases: `in_flight > 0` or
`next_batch.is_some()`. When both are false (arm 3 stuck), arm 4 fired silently. This is why
18:28–SIGTERM produced no stall warnings despite the driver being completely frozen.

**Fix applied** (`lib/vector-stream/src/driver.rs`): added a third `else` branch to arm 4 that
logs `"Input stream has not produced a batch; possible upstream pipeline stall."` with
`stalled_secs`. Future occurrences will be immediately visible in logs.

### The 18:24–18:27 warnings were false alarms

Each of those stall warnings had `stalled_secs=0`, meaning `last_completion.elapsed()` was
essentially zero — requests were completing normally. They indicate transient in-flight request
pauses, not the deadlock we're investigating.

### Back-pressure followed from the stall, it did not cause it

The Kafka source stopped at ~18:30, after the driver was already frozen. The consumer group lag
grew as a result of the pipeline stall, not as a precondition.

---

## What We Have Ruled Out

### ConcurrentMap waker correctness (upstream PR already deployed)

The upstream `ConcurrentMap` had a known issue (fixed in vectordotdev/vector#23340, already
deployed in our build): when the underlying stream was exhausted, `poll_next` would return
`Poll::Pending` instead of `Poll::Ready(None)`, hanging shutdown. That fix is present in our
codebase (`None if this.stream.is_done() => Poll::Ready(None)`).

Two regression tests were added to `lib/vector-stream/src/concurrent_map.rs` to verify the two
waker paths in `ConcurrentMap`:

1. **`test_item_delivered_when_upstream_ready_while_at_limit`** — verifies that an item sent to
   the upstream channel while `in_flight.len() == limit` (the "waker gap") is eventually delivered
   after the in-flight task completes and the next `poll_next` re-registers with upstream.

2. **`test_item_delivered_after_pending_with_empty_in_flight`** — verifies that an item sent after
   `ConcurrentMap` parks on an empty `FuturesOrdered` queue (returning `Poll::Pending` via the
   `ready!` macro) is delivered when upstream fires.

Both tests pass. The waker gap is a latency issue, not a permanent hang — items are not lost, they
are just delayed until the next `poll_next` call re-registers. This is consistent with the 18:28Z
transition (normal operation → stuck), not with a theoretical waker starvation.

---

## Open Questions / Candidates for Root Cause

### 1. Batcher timer waker starvation

The `Batcher` accumulates events and flushes on a timer or when the batch reaches a size threshold.
If no events arrive and no timer fires, `Batcher::poll_next` returns `Poll::Pending`. The question
is whether the timer can get into a state where it is never re-armed after a flush, leaving the
`Batcher` permanently pending even when events exist upstream.

**How to test:** instrument `Batcher::poll_next` with stall-detection metrics; look for the
combination of "Kafka is producing" + "Batcher is Pending" in the same time window.

### 2. `ready_chunks` accumulation behavior

`ready_chunks(1024)` wraps the `ConcurrentMap` output. If `ConcurrentMap::poll_next` returns
`Poll::Ready(None)` prematurely (stream closed), `ready_chunks` would flush and then terminate.
If it returns `Poll::Pending` with items buffered but no waker registered, it could stall.

### 3. Kafka source back-pressure loop

The Kafka consumer may have stopped producing events into the channel feeding the pipeline
before the GCL sink stalled — not because of GCL back-pressure, but due to an independent
consumer-side issue (e.g., rebalance, partition assignment change, or a channel buffer full
event that was never drained). This would make the stall appear at arm 3 but originate in the
source.

**Evidence needed:** Kafka consumer lag metrics and consumer group events in the 18:24–18:28
window, specifically whether lag was growing before or after 18:28.

### 4. Hyper/h2 connection state

The GCL sink uses HTTP/2 to Google APIs. If the h2 connection entered a state where no new
streams could be opened (e.g., `MAX_CONCURRENT_STREAMS` reached, GOAWAY received, or a
connection-level flow-control deadlock), the service's `poll_ready` would return `Pending`
indefinitely. The driver would then sit on arm 2 with `next_batch=Some(...)`, not arm 3.

This is **inconsistent with the observed state** (`next_batch=None`), so an h2 connection
deadlock alone cannot explain the arm 3 stall. However, a prior h2 deadlock could have consumed
the last in-flight requests and then resolved, leaving the driver with `in_flight=0` and a
stalled upstream stream — consistent with the transition at 18:27:19.

---

## Instrumentation Added

All changes live in the `crl-debug` branch of this repository, generated from the patched
`/tmp/vector-src` tree and captured in `docs/gcl-sink-stall-investigation.md` (this file).

### `lib/vector-stream/src/driver.rs`

- **Stall detection v4:** arm 4 now covers all three states (`in_flight > 0`, `next_batch.is_some()`,
  and the new case: both false → "Input stream has not produced a batch").
- **`async_backtrace` frames:** arm 2 (`poll_ready`) and arm 3 (`batched_input.next()`) are wrapped
  in `async_backtrace::location!().frame(...)`. On SIGTERM, the task dump identifies which arm the
  driver is suspended in by name. In-flight HTTP requests are also framed so their suspend point
  (retry backoff, hyper send, response read) is visible in the dump.

### `src/debug_dump.rs` + `src/signal.rs` + `src/app.rs`

- On SIGTERM, an `AtomicBool` flag is set in the signal handler (signal-safe).
- A background task polls the flag every 50ms and calls `do_dump()` asynchronously.
- `do_dump()` writes to stderr: process status, open FDs, per-thread `/proc` info (wchan,
  syscall), current thread backtrace, async task tree (via `async_backtrace::taskdump_tree`),
  and Tokio runtime metrics (worker queue depths, steal counts, poll counts).

### `src/http.rs`

- HTTP/2 keepalive and ping settings configured to surface stalled connections faster.

### `lib/vector-stream/src/concurrent_map.rs`

- Two regression tests for the at-limit and empty-in-flight waker paths.

---

## Configuration Tweaks Eliminated

The following GCL sink configuration values have been adjusted during the investigation. None
eliminated the stall. The current production values (in
`environments/prod/pipeline_configs/sinks/google_cloud_logging.toml`) reflect the latest state
after these experiments.

Upstream issue tracking this class of problem: **vectordotdev/vector#25217**

### Batch sizing

```toml
batch.max_events = 256   # tried; current prod value is 512
batch.max_bytes  = 4194304  # 4 MB tried; current prod value is 8388608 (8 MB)
batch.timeout_secs = 1   # unchanged throughout
```

**Rationale for trying smaller batches:** A large batch that hits the GCL payload size limit
returns HTTP 400. Reducing `max_events` and `max_bytes` was intended to keep individual requests
well under the API limit and reduce per-request latency, which might prevent the service layer
from backing up. The `timeout_secs = 1` flush was already in place to prevent low-throughput
periods from holding partial batches indefinitely.

**Why eliminated:** The stall occurs with `in_flight=0` and `next_batch=None`. The driver is not
blocked waiting for a service response or holding an oversized batch — it is stuck before a
batch is even assembled. Batch sizing affects what happens after `batched_input.next()` yields
a value; it has no bearing on why that future becomes permanently Pending.

### Request concurrency

```toml
request.concurrency = 10  # tried; current prod value is commented out (adaptive)
```

**Rationale for trying fixed concurrency:** The adaptive concurrency controller (AIMD algorithm)
adjusts the in-flight request limit based on observed latency. Under load it could oscillate
or drive concurrency to 1, creating a head-of-line blocking situation. Pinning to 10 was meant
to remove that variable and provide a stable number of concurrent GCL streams.

**Why eliminated:** Again, the stuck state has `in_flight=0`. The concurrency controller governs
how many requests can be in-flight simultaneously via `poll_ready`; when there are zero in-flight
requests the controller is irrelevant. A concurrency limit of 1 or 100 makes no difference if
`batched_input.next()` never returns a batch to submit. Current prod config leaves concurrency
adaptive (commented out) since fixed concurrency at 10 showed no improvement.

### Request timeout and retry

```toml
request.timeout_secs  = 30   # unchanged throughout
request.retry_attempts = 7   # unchanged throughout
```

These values were reviewed but not changed. A 30-second timeout per request and 7 retry attempts
with Fibonacci backoff means a single request can consume several minutes before being dropped.
Under the stall scenario this is moot (no requests are in flight), but under heavy load these
settings affect how long the driver spends in arm 2 waiting for retried requests to clear before
arm 3 can fetch the next batch. No evidence that adjusting them changes stall frequency.

---

## Environmental Patterns

### Load correlation

The stall is strongly correlated with weekday business-hours traffic. It is consistently observed
between roughly **10:00–17:00 EST on weekdays** and has not been observed on weekends.

**Autoscaling rules out per-pod event rate as the distinguishing variable.** The cluster uses
horizontal pod autoscaling, which adds pods to keep per-pod CPU utilization at a fixed target.
This means pods on weekdays and weekends are targeted to the same events/s rate. The weekday-only
correlation therefore cannot be explained by volume alone — something about the *character* of
weekday events differs from weekend events.

Likely distinguishing factors:

- **Log content complexity.** Weekday business-hours traffic routes through more of the 54-transform
  chain (different `route_*` branches, more `grok_*` parses, more `parse_regex` hits). Each event
  consumes more CPU per unit even at the same ingress rate. This increases Tokio runtime contention
  without changing events/s — consistent with a race-condition trigger that is sensitive to
  scheduler pressure rather than throughput.
- **GCP-side behavior.** GCL API quota pressure, h2 connection churn, or GOAWAY frames may be more
  frequent during business hours when other Etsy services are also writing to Cloud Logging.
- **Kafka partition activity.** More topic partitions may be actively producing on weekdays, changing
  the consumer's poll batching behavior and the inter-arrival time distribution of events at the
  channel feeding the Batcher.

The practical implication: **increasing per-pod CPU utilization target** (by raising the autoscaler
threshold) would increase Tokio scheduler contention without raising events/s, which — if the bug
is a waker registration race — would make the stall more frequent. See the race condition analysis
in the "Race condition hypothesis" section below.

### Scope — multiple pods, multiple stalls per hour

Multiple pods can stall within the same hour. **Newly started pods are also affected** — a fresh
pod with no prior state can deadlock within minutes of its first traffic. This rules out any
theory that requires accumulated state, long-lived connection degradation, or a slow memory leak.
Whatever triggers the stall can manifest immediately at normal operating load.

The stall reproduces in **both dev and prod** environments, so it is not unique to a specific
cluster configuration, Kafka topic, or GCP project.

### Recovery without restart

The stall is not always permanent. In some instances the driver has self-recovered after hours
without a pod restart, implying the underlying waker or connection eventually fires on its own.
This is consistent with a very long timeout or a GCP-side keepalive eventually resetting the h2
connection and unblocking `poll_ready` — though the driver's observed state at SIGTERM
(`next_batch=None`) suggests the block is on the input stream side, not the service side.

The possibility of eventual self-recovery means the stall is a liveness issue (forward progress
halts for hours) rather than a permanent hard deadlock.

### Automated mitigation: pod deletion every 15 minutes

`scripts/delete-deadlocked-vector-pods.sh` runs on a 15-minute cron and SIGTERMs any pod
matching the deadlock heuristic. The detection criteria (from the script's PromQL) are:

1. **GCL send rate is zero** for the full 10-minute check window (`max_over_time(...)[10m:]` == 0)
2. **Kafka source buffer utilization stays above 10,000 events** for the same 10 minutes
   (`min_over_time(...)[10m:]` > 10,000)

A pod must satisfy both conditions simultaneously — zero GCL output while the upstream Kafka
buffer is clearly full — before it is deleted. This prevents false positives from pods that are
legitimately idle (zero send rate AND low buffer). The script uses `kubectl delete pod` (graceful
termination), which sends SIGTERM before SIGKILL, allowing the v4 debug dump to fire.

The 15-minute window means a stalled pod can go undetected for up to **25 minutes** (10-minute
confirmation window + up to 15 minutes until the next cron run). At that point the pod is
replaced, but the root cause remains unfixed for the next pod.

---

## Building the v4 Debug Image

The image is built from this repository using `Dockerfile.debug`. It targets `linux/amd64` and
produces a `debian:12-slim`-based image (not distroless — the dynamically linked binary requires
`libssl`, `libsasl2`, and other system libraries that distroless omits).

**Registry:** `us-central1-docker.pkg.dev/etsy-buildkite-prod/edc/vector`
**Tag:** `0.53.0-debug-sigterm-async-bt-h2ping-v4`

### Prerequisites

```bash
# Authenticate Docker to the Artifact Registry
gcloud auth configure-docker us-central1-docker.pkg.dev

# Ensure Docker is running and BuildKit is available (default on Docker >= 23)
docker version
```

The `.dockerignore` at the repo root already excludes `target/` and `.git/` to keep the build
context small (~50 MB of source vs ~2 GB with artifacts).

### Build

From the root of this repository (`~/code/vector-src`):

```bash
IMAGE=us-central1-docker.pkg.dev/etsy-buildkite-prod/edc/vector
TAG=0.53.0-debug-sigterm-async-bt-h2ping-v4

docker build \
  --platform linux/amd64 \
  -f Dockerfile.debug \
  -t "${IMAGE}:${TAG}" \
  .
```

The build takes 20–40 minutes on a laptop (full Rust compile from source, no layer cache on first
run). Subsequent builds reuse the `apt-get` and dependency-compilation layers if only source files
changed.

### Push and Capture the Digest

```bash
IMAGE=us-central1-docker.pkg.dev/etsy-buildkite-prod/edc/vector
TAG=0.53.0-debug-sigterm-async-bt-h2ping-v4

docker push "${IMAGE}:${TAG}"
```

Docker prints the digest on the final push line:

```
0.53.0-debug-sigterm-async-bt-h2ping-v4: digest: sha256:<64-hex-chars> size: ...
```

To retrieve it after the fact:

```bash
docker inspect --format='{{index .RepoDigests 0}}' "${IMAGE}:${TAG}"
# or
docker buildx imagetools inspect "${IMAGE}:${TAG}" | grep Digest
```

The canonical reference for the deployment manifest is the digest-pinned form:

```
us-central1-docker.pkg.dev/etsy-buildkite-prod/edc/vector@sha256:<digest>
```

### Deploy to Dev

Update `~/code/vector/lib/images.json` (the `vectordev` entry) with the new tag to roll out to
the dev environment first:

```json
"vectordev": {
  "image": "us-central1-docker.pkg.dev/etsy-buildkite-prod/edc/vector",
  "version": "0.53.0-debug-sigterm-async-bt-h2ping-v4",
  "mirror": "us-central1-docker.pkg.dev/etsy-buildkite-prod/edc/vector"
}
```

---

## Race Condition Hypothesis

### Evidence for a race condition rather than a logic error

The rd9bh pod ran for over an hour (15:45–16:55 UTC) during which arm 4 fired dozens of times in
the `in_flight=0, next_batch=None` state (`stalled_secs=0`) and recovered each time — meaning
`batched_input.next()` correctly registered its waker on every one of those calls. The permanent
stall occurred on one specific call where it did not. This intermittency is the hallmark of a race
condition: the same code path succeeds under normal scheduling and fails under a specific
interleaving.

A pure logic error (wrong branch taken based on deterministic state) would fail consistently once
the state is reached, not intermittently across dozens of successful iterations.

### The TOCTOU waker race pattern

Async waker correctness requires avoiding a time-of-check / time-of-use (TOCTOU) gap:

```
Thread A (poller):           Thread B (producer):
1. check: no data yet
                             2. data arrives, fire waker → no waker stored yet, noop
3. store waker
   [waker never fired again]
→ permanent Poll::Pending
```

Tokio's `mpsc` channel uses `AtomicWaker` which handles this with a two-phase store + re-check.
However, custom stream combinators (`Batcher`, `ConcurrentMap`) that wrap the channel introduce
their own state machines. If any wrapper layer has a TOCTOU gap between "upstream returned Pending"
and "waker stored in cx by upstream poll", a producer wake that fires between those two events is
silently lost.

### CPU utilization target as a race amplifier

Higher CPU utilization means Tokio's worker threads are more frequently preempted and rescheduled.
This widens every TOCTOU window — the gap between step 1 and step 3 above becomes longer in wall
time, giving step 2 more opportunity to occur in between.

**Prediction:** increasing the autoscaler's CPU utilization target (causing each pod to run hotter)
would make the stall **more frequent** if the root cause is a waker TOCTOU race. It would have
no effect if the root cause is a deterministic logic error or a Kafka source issue.

This prediction is testable: deploy v5 to dev, raise the CPU target, and observe whether stall
rate increases. A confirmed increase is strong evidence for a race condition, and rules out
Kafka-source and logic-error hypotheses.

### Implication for the weekday correlation

Weekday business-hours events are more parse-intensive (more grok paths, more VRL remap work per
event). This raises per-event CPU cost without raising events/s (which autoscaling normalizes).
The result is higher Tokio runtime contention at the same throughput — consistent with a race
condition that is sensitive to scheduler pressure rather than raw event volume.

---

## Findings from v4/v5 Debug Images (2026-04-21)

### Cross-pod validation: cnp24, rd9bh, 928m7

Three pods stalled and were SIGTERM'd on 2026-04-21. All three show identical behavior:

| Pod | Last arm-1 tick | Persistent arm-3 starts | stalled_secs at SIGTERM | async_backtrace suspension |
|---|---|---|---|---|
| cnp24 | 17:13:34 (in_flight=3) | 17:14:34 | 916s | `driver.rs:208:30` |
| rd9bh | 16:54:33 (in_flight=2) | 16:55:33 | 1226s | `driver.rs:208:30` |
| 928m7 | 17:54:33 (in_flight=1) | 17:55:33 | 1230s | `driver.rs:208:30` |

The async_backtrace Driver frame is byte-for-byte identical across all three pods: arm 3,
`batched_input.next()` at `driver.rs:208:30`. This is confirmed, not a hypothesis.

### "Source send cancelled" timing

| Pod | Arm-3 stall start | Source send cancelled | Lag |
|---|---|---|---|
| cnp24 | ~17:14:34 | 17:16:21 | ~107s |
| rd9bh | ~16:55:07 | 17:15:08 (at SIGTERM) | ~20 min (buffers held) |
| 928m7 | ~17:55:03 | 17:57:17 | ~134s |

"Source send cancelled" is a backpressure consequence, not the cause. In rd9bh it did not appear
until pod termination (upstream buffers absorbed 20 minutes of backpressure), confirming it is not
on the causal path to the stall.

### Batcher waker static analysis

Static analysis of `Batcher::poll_next` found all three `Poll::Pending` sub-paths correctly
register wakers. No logic error was identified. Three regression tests were added
(`batch_delivered_after_timer_flush_and_dormant_period`,
`upstream_waker_registered_when_timer_none_and_batch_empty`,
`second_batch_delivered_after_timer_flush_during_at_limit_gap`) to guard these paths.

The v5 image adds named `async_backtrace` frames inside `Batcher::poll_next` and
`ConcurrentMap::poll_next`. The next SIGTERM dump will resolve which of four specific await points
is the leaf, identifying the exact layer where waker registration fails.

---

## Next Steps

1. **Deploy v5 image** (`0.53.0-debug-sigterm-async-bt-h2ping-v5`) to dev and wait for next stall.
2. **On next SIGTERM dump:** look for the leaf frame — one of:
   - `Batcher::poll_next - waiting for timer` (`batcher/mod.rs:122`) → timer waker issue
   - `Batcher::poll_next - waiting for upstream` (`batcher/mod.rs:137`) → channel empty; Kafka source stopped before stall
   - `ConcurrentMap::poll_next - waiting for in-flight` (`concurrent_map.rs:106`) → stall above Batcher
   - `ConcurrentMap::poll_next - waiting for upstream` (`concurrent_map.rs:125`) → ConcurrentMap upstream polling issue
3. **If "waiting for upstream":** correlate with Kafka consumer lag metrics at stall onset to
   confirm whether the source stopped before or after the driver froze.
4. **Race condition test:** raise the dev autoscaler CPU utilization target and observe whether
   stall frequency increases. An increase confirms race-condition hypothesis and rules out
   deterministic logic error and pure Kafka-source theories.
5. **If source was producing at stall onset:** the bug is a waker TOCTOU race inside the Batcher
   or ConcurrentMap chain; add `AtomicWaker` audit or targeted synthetic reproducer under load.
