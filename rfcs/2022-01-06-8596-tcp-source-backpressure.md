# RFC 8596 - 2022-01-06 - TCP Source Backpressure

Backpressure is necessary in order to limit the total number of events that Vector has to hold when a sink is
unable to process as quickly as a source accepts new events.

TCP sources today are processed on a per-connection basis. For each connection, a request is decoded, and all the events
contained in that request are sent to the pipeline. Backpressure is applied when flushing events into the pipeline.
When this happens, it prevents that one specific connection from decoding another request until there is enough
room in the pipeline for flushing to complete. It does not, however, prevent new connections from being opened,
which will decode another request. If the number of connections are unbounded, then the total number of events that
Vector has to hold in memory is also unbounded, causing excessive memory usage, or crashing Vector entirely.

The goal here is to set an upper bound on the number of events held by Vector by propagating backpressure
to the source of events.

## Context

- [add config option to limit source tcp connections (off by default)](https://github.com/vectordotdev/vector/pull/10491)
- [Epic for source backpressure handling in all sources](https://github.com/vectordotdev/vector/issues/8820)

## Cross cutting concerns

## Scope

### In scope

- Limit the number of in-flight events for TCP sources.
  - fluent
  - socket (tcp mode only)
  - logstash
  - statsd
  - syslog

- Reduce the number of config options required to solve this where possible (it should work by default)
- Prevent performance regressions from any necessary changes

### Out of scope

- Non-TCP based sources

## Pain

It may be more clear why this is a problem if we look at a specific example. The `fluent` source collects logs from
`fluentd` over TCP connections. For this source, the number of events held by Vector is
`smallest buffer size of connected sinks [default: 500] + pipeline buffer [1000] + (# of events in a request * # of TCP connections)`

`fluentd` can send batches of over 20,000 events in a single request. It will also
utilize more TCP connections if it is unable to send fast enough. If the sink is unable to keep up, there will be
an entire request's worth of events in memory per TCP connection, and `fluentd` will keep opening more. This
can quickly exhaust all the available memory. Even if memory doesn't run out and cause Vector to crash, each
TCP connection that opens will compete with the existing connections trying to send events to the pipeline. Each
connection will take longer and longer to process until they start timing out and `fluentd` retries those events,
so you end up making no forward progress at all.

## Option 1 (static connection limit)

The naive solution is to just pick static connection limit for each source. This would fix the issue because the
number of connections is bounded, so the number of requests in-flight would also be bounded.

Opt-in static connection limits have already been added to Vector. This option is proposing to make it a default.

### Rationale

- Easy to implement
- Effective

### Drawbacks

- If this is a hard-coded limit, the maximum throughput of Vector may be limited.
- If this is selected by the user, the user would need to understand how to set this value, and update it when needed.

## Option 2 (dynamic connection limit)

This removes both drawbacks from Option 1 by having Vector itself dynamically adjust the limit. The question now becomes,
how do you select that value?

The most similar thing already in Vector is ARC (Adaptive Request Concurrency). However, this same algorithm is not
appropriate for TCP connections. ARC uses the "additive increase / multiplicative decrease" algorithm to frequently adjust
how many requests are in-flight at the same time. Each request generally has a similar number of events and occur
frequently enough that the number in-flight can be adjusted quickly.

With TCP connections, it is impossible to know ahead of time how many events a TCP connection will send before accepting it.
You also don't know how long a connection may live. It could be a static connection that stays open long-term, or it could send just a single event
then close. It is also generally not possible to forcibly close a connection without dropping events, since most of the
TCP sources do not have a way to send an "ack" in the protocol.

I believe this option is not feasible. Trying to dynamically limit requests in flight by only choosing when
to accept a new connection does not give us enough control to both limit the number of in-flight requests
and also maintain acceptable performance in all cases.

## Option 3 (dynamic request limit)

Instead of trying to control the number of in-flight requests at the connection level, the requests can just be controlled directly.
The number of TCP connections could stay unlimited, but there is a check before each request is processed that
can limit how many are actually in-flight. The goal here is to pick a limit to the number of in-flight requests and
only allow new ones to be processed if you are below that. This can't be limited to an _exact_ number of events, because you
don't know how many events are in a specific request until it has been decoded. But you can use a limit in the form of
`x events + y requests`. This would attempt to limit the in-flight events to `x`, plus `y` full requests of events.

The main drawback to this approach is that you have to be able to accept a request before you know how many events
are in that request, and the total in-flight count can't be updated until after the decoding has finished. A concurrency
limit must be placed on request decoding (equal to `y` above) in order to limit the overall requests in-flight.

This leads us to the question of what should the value of `x` and `y` be? `x` (the number of events in-flight) can probably
be set to 0 and ignored, since we already have event buffering in other parts of the system (the pipeline and sink buffers).
Choosing `y` is balancing performance with memory usage.

I propose that `y` is a value dynamically determined by the request size of previous requests.
The "request size" will simply be defined as the number of events contained in a request. The running-average of
these will be used to estimate the size of future requests. Now a target number of in-flights requests can be chosen.
`y` will be set to a value such that actual in-flight messages + estimated in-flight messages is less than the target.
Once a request is decoded the actual in-flight messages can be increased, and as they are flushed into the pipeline
the count will be decreased. The target value will be a constant value somewhat arbitrarily chosen based on perf tests.
The exact value will be proposed in the final implementation.

A dynamic value for `y` is useful because the size of the requests can vary drastically depending on the source / protocol used.
Each request may only contain a single event, or as noted above can contain tens of thousands. Using a constant
value would likely penalize sources using small requests by artificially limiting concurrency, or allowing
sources with large requests to have too many events in memory.

On top of the dynamic value for `y`, some hard limits will be added. Regardless of the current request size estimate,
1 request must always be allowed to be processed when the actual in-flight message count is zero to ensure forward
progress can be made. In addition, an upper-bound will be set equal to the number of CPU's on the system. A
value higher than this has no benefit, but would use more memory.

### Rationale

- No user facing config needed, this will work by default.

### Drawbacks

- More complicated to implement than option 1.
- Requires choosing an appropriate "target" value. If too low, it could limit overall throughput. If too high, it could use too much memory.

## Proposal

I am proposing implementing option 3 (dynamic request limit).
This will limit the number of requests being processed concurrently to balance memory usage with performance.
I don't think option 2 (dynamic connection limit) is feasible to implement, and option 1 (static connection limit) is
too difficult for users to configure.

### User Experience

- A concurrency limit will be applied to request decoding to ensure that TCP-based sources propagate backpressure appropriately, and don't consume too much memory.

## Prior Art

- ARC is similar, but is ultimately solving a different problem and doesn't seem applicable here
- Most other systems I'm aware of use configurable limits (or limits per "user"), which is option 1 above

## Outstanding Questions

TBD

## Plan Of Attack

- [ ] Prototype option 3 to get initial performance numbers and make sure it is feasible
- [ ] Submit the full PR with the changes

## Future Improvements
