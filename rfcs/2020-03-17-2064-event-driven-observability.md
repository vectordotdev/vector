# RFC 2064 - 2020-03-17 - Event-driven Observability

This RFC proposes a new API wrapping both `tracing` and `metrics` that
encourages strongly-typed events.

## Motivation

As we take the time to instrument every component within Vector, we want to make
sure that this instrumentation is **consistent**, **concise**, **thorough**, and
**discoverable**.

This is a challenge with existing tools for a number of reasons. Consider
the following example (taken from the [file source][0]):

```rust
messages
    .map(move |(msg, file): (Bytes, String)| {
        trace!(
            message = "Received one event.",
            file = file.as_str(),
            rate_limit_secs = 10
        );
        counter!("events_processed_total", 1, "source" => "file");
        counter!("bytes_processed", msg.len() as u64, "source" => "file");
        create_event(msg, file, &host_key, &hostname, &file_key)
    })
    .forward(out.sink_map_err(|error| error!(?error)))
```

It's immediately obvious that we're trading off brevity for thoroughness. There
are three separate statements related to observability, taking up more lines
than the actual logic. While it's good to be thorough, this is a significant
disruption both for readers and the contributor writing the code.

The less obvious challenges are consistency and discoverability. Each of the
APIs in use (i.e. `trace!` and `counter!`) are designed around strings and
arbitrary key/value pairs. While it might be simple enough to keep those
consistent when they're nearby in the same file, doing so across all of our
components poses a significant challenge. If nothing else, it's a mental burden
for developers to maintain manually.

Short of `grep`, there is no easy way to catalog and display these
instrumentation points to users. Particularly for metrics, this can be very
valuable.

## Guide-level Proposal

At any point in the code where something happens that a user might want to
collect data about (i.e. an _event_ occurs), instead of reaching for our usual
log statements and metrics collectors, we should define a new internal event.
This doesn't necessarily include one-time log messages around startup or
shutdown, but does include all metrics collection and log events that can happen
repeatedly at runtime.

Internal events are simply structs which implement the `InternalEvent` trait.
The idea is quite simple:

1. Go to the top-level `internal_events` module and define a new struct with
   fields for whatever relevant data you have.

2. Implement the `InternalEvent` trait for your new struct. There are two
   methods, `emit_logs` and `emit_metrics`, and both default to doing nothing.
   Fill them in with the relevant logging and/or metrics instrumentation your
   event should translate into.

3. Back at your instrumentation site, use the `emit!` macro to register a fresh
   instance of your new event. This takes care of expanding it into all the log
   and metrics calls you defined in the previous step.

Using the example from the previous section, the code may now looks something
like this:

```rust
messages
    .map(move |(msg, file): (Bytes, String)| {
        emit!(FileEventReceived {
            file: file.as_str(),
            byte_size: msg.len(),
        });
        create_event(msg, file, &host_key, &hostname, &file_key)
    })
    .forward(out.sink_map_err(|error| error!(?error)))
```

And in the `internal_events` module, we would add the following:

```rust
pub struct FileEventReceived<'a> {
    pub file: &'a str,
    pub byte_size: usize,
}

impl InternalEvent for FileEventReceived<'_> {
    fn emit_logs(&self) {
        trace!(
            message = "Received one event.",
            file = self.file,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("events_processed_total", 1, "source" => "file");
        counter!("bytes_processed", self.byte_size as u64, "source" => "file");
    }
}
```

Some specific notes on the implementation:

* Both event struct names and metric keys follow the `{noun}_{verb}` naming
  scheme. This keeps things simple, and we can rely on tags/labels for more
  context.

* The event struct avoid any expensive operations like string allocation, etc.
  By focusing on small integers and existing string slices we can minimize the
  performance impact of our instrumentation.

* While events could technically use a constructor or other method of
  initialization, we prefer the simple struct literal method. This maintains the
  familiar and easy-to-read key/value format of the underlying APIs while using
  the compiler to inform us of any missing fields or mismatched types. See the
  [init struct pattern][1] for more discussion.

## Prior Art

This work is based primarily on the implementation within Timber's closed-source
hosted service (https://timber.io). We can't use it because it's written in
Elixir.

It also builds on top of the excellent work done on the `tracing` and `metrics`
crates. While it presents a different API for contributors, it still uses the
full flexibility and performance of these great libraries under the hood.

## Sales Pitch

This approach effectively separates concerns, keeping instrumentation points
concise while giving us a dedicated place to make the data consistent, thorough,
and discoverable.

More specifically:

* Using simple structs as the API leverages the compiler to enforce a consistent
  set of keys and values everywhere an event is emitted.

* Removing metrics derivation from the normal flow of code encourages
  contributors to collect more thorough data.

* Putting all events in a single module makes them easily to audit for
  consistency.

* The `InternalEvent` trait gives us a place to enforce uniform implementation
  within events.

* Being a thin layer over the existing `tracing` and `metrics` APIs should allow
  most, if not all, of this additional code to be inlined or optimized away,
  leaving us with the same excellent performance as the underlying libraries.

## Drawbacks

The primary downside of this approach is that it is unfamiliar to most
developers and adds an additional step to the development process. We would need
to take steps in documentation, API design, and code review to reduce friction
for contributors as much as possible.

## Alternatives

The most realistic alternative is to simply not build this additional layer and
use the normal `tracing` and `metrics` APIs. This has the benefit of familiarity
and ease of use, but also brings all the downsides discussed above.

Another alternative is to build tooling to help enforce consistency, provide
discoverability, etc while using the normal APIs. This would be challenging to
do in a precise way, and would not provide the benefits of separating concerns
we'd get with explicit events. Since the convention would not be enforced by the
compiler, the feedback loop would also likely be longer.

## Outstanding Questions

### Specific vs Reusable Events

Consider the difference between the `FileEventReceived` event in the example
above and something like the following:

```rust
pub struct EventReceived {
    pub source_type: &'static str,
    pub byte_size: usize,
}
```

Or even:

```rust
pub struct EventProcessed {
    pub component_kind: &'static str,
    pub component_type: &'static str,
    pub byte_size: usize,
}
```

On one hand, more specific events like `FileEventReceived` can be tightly
tailored to a specific callsite with data like `file`. On the other, they have
a higher cognitive cost since they need to be implemented over and over for
different sources, etc. This also makes consistency slightly more of
a challenge.

The more generic events can be quickly, easily, and consistently used across
different components, but can be more verbose and include less contextual
information. Extending them to allow for more dynamic data would defeat much of
the purpose of using structs in the first place.

Finding the right middle ground here is something that will likely come with
experience, but it's worth thinking about some general guidelines from the
start.

### Collecting Uniform Context Data

This challenge is not specific to the event struct approach, but it very much
affects how we go about designing them.

The core of the issue is that we want a consistent set of context fields to use
for tags/labels in metrics, but that data is not always readily available in the
code itself (e.g. configured name of the current component). We also don't want
to have to thread this data through every API and shared components.

The `tracing` crate solves this problem for logging with the idea of spans. We
can easily attach fields to all log events that get emitted within a particular
span's task.

I believe our goal here should be something like the following:

1. Settle on a set of well-known pieces of context that we ensure are set
   uniformly on spans at the topology-building layer (e.g. `component_kind`,
   `component_type`, `component_id`).

2. Rely on the existing `tracing` implementation to output that context in logs.

3. In parallel with the main instrumentation work, pursue a `metrics`
   implementation that integrates with `tracing` and can access these well-known
   keys on surrounding spans and apply them as labels.

This means our initial data will not be organized by all the labels we'd ideally
prefer, but gives us a path to adding them later with no changes to each
callsite. That work is also effectively decoupled from event implementations.

### Implementation specifics

The example above uses an `emit!` macro, but doesn't currently do anything that
requires it to be a macro. This may provide some flexibility for the future, or
could be considered an over-complication.

The example also splits the `InternalEvent` trait into two method, `emit_logs`
and `emit_metrics`. There's no strong need for this split, since both are simply
called one after another when an event is emitted. It could be simpler to
provide one required method instead of these two optional ones.

## Plan Of Attack

Instrumentation work:

* [ ] Update #1953 to be consistent with this RFC and serve as concrete examples
  for the discussion
* [ ] Once we've reached consensus, merge #1953
* [ ] Update #2007 with a list of remaining events to be implemented
* [ ] Implement the remaining events in chunks, grouped by component

Metrics context work:

* [ ] Coordinate with `metrics` maintainers to determine best path forward for
  context data (see [relevant issue][2])

[0]: https://github.com/vectordotdev/vector/blob/8140c2b6b04509ffc669aeefbf3c2a07c1b246d1/src/sources/file/mod.rs#L285-L291
[1]: https://xaeroxe.github.io/init-struct-pattern/
[2]: https://github.com/metrics-rs/metrics/issues/67
