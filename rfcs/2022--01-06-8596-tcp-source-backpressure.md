# RFC 8596 - 2022-01-06 - TCP Source Backpressure

Backpressure is necessary to be able to limit to total number of events that Vector has to hold in the event
that a sink is unable to process as quickly as a source accepts new events. Today backpressure is applied when events
are flushed in the pipeline. If the pipeline buffer is full when flushing (default size is 1000), then flushing
will block until there is enough room.

TCP sources spawn a new tokio task to handle each connection. Each task generally decodes a single request and
sends that to the pipeline. Backpressure will be correctly applied per connection since flushing into the pipeline
will prevent a new request from being read off of the connection. However, if the number of connections is unbounded
, then the total number of events that Vector has to hold on to is also unbounded.




## Context

- [add config option to limit source tcp connections (off by default)](https://github.com/vectordotdev/vector/pull/10491)

## Cross cutting concerns

- Link to any ongoing or future work relevant to this change.

## Scope

### In scope

- Limit the number of in-flight events for TCP sources.
- Reduce the number of config options required to solve this where possible (it should work by default)
- Prevent performance regressions from any necessary changes

### Out of scope


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
connections is bounded, so the number of requests in-flight is now also bounded.

Opt-in static connection limits has already been added to Vector. This option is proposing to make it a default.

### Rationale
- Easy to implement
- Effective

### Drawbacks
- If this is a hard-coded limit, the maximum throughput of Vector may be limited.
- If this is selected by the user, the user would need to understand how to set this value, and update it when needed.


## Option 2 (dynamic connection limit)

This removes both drawbacks from Option 1 by having Vector itself dynamically adjust the limit. The question now becomes,
what value do you select.

The most similar thing already in Vector is ARC (Adaptive Request Concurrency). However, this same algorithm is not
appropriate for TCP connections. ARC uses the "additive increase / multiplicative decrease" algorithm to frequently adjust
how many requests are in-flight at the same time. Each request generally has a similar number of events and occur
frequently enough that the number in-flight can be adjusted quickly. With TCP connections, it is impossible to know
ahead of time how many events a TCP connection will send before accepting it. You also don't know how long
a connection may live. It could be a static connection that stays open long-term, or it could send just a single event
then close. It is also generally not possible to forcibly close a connection without dropping events.

I believe this option is not feasible. Trying to dynamically limit requests in flight by only choosing when
to accept a new connection does not give us enough control to both limit the number of in-flight requests
and also maintain acceptable performance in all cases.

## Option 3 (dynamic request limit)

Instead of trying to control the number of in-flight requests at the connection level, you can just control them directly.
The number of TCP connections could stay unlimited, but there is a check before each request is processed that
can limit how many are actually in-flight. The goal here is to pick a limit to the number of in-flight requests and
only allow new ones to be processed if you are below that. You can't limit this to an _exact_ number of events, because you
don't know how many events are in a specific request until it has been decoded. But you can use a limit in the form of
`x events + y requests`. This would attempt to limit the in-flight events to `x`, plus `y` full requests of events.

The main drawback to this approach is that you have to be able to accept a request before you know how many events
are in that request, and the total in-flight count can't be updated until after the decoding has finished. A concurrency
limit must be placed on request decoding (equal to `y` above) in order to limit the overall requests in-flight.

This leads us to the question of what should the value of `x` and `y` be? `x` (the number of events in-flight) can probably
be set to 0 and ignored, since we already have event buffering in other parts of the system (the pipeline and sink buffers).
Choosing `y` is balancing performance with memory usage.

I propose that `y` is a small, hard-coded value. (Exact value is TBD)
Decoding events is generally pretty quick, and there are already single-threaded bottlenecks in sources today. A
small hard-coded value will likely not cause any significant performance impacts (although this will be tested).
As the architecture of Vector changes, this can be adjusted as needed. I do not think users should have to understand
this or need to worry about setting it.


### Rationale
- No user facing config needed, this will work by default.
- This will effectively propagate backpressure / limit the number of in-flight events.

### Drawbacks
- More complicated to implement than option 1.
- Requires choosing an appropriate value for `y`. If too low, it could limit overall throughput. If too high, it could use too much memory.


## Proposal

I am proposing to implement option 3 (dynamic request limit).
This will limit the number of requests being processed concurrently to balance memory usage with performance.
I don't think option 2 (dynamic connection limit) is feasible to implement, and option 1 (static connection limit) is
too difficult for users to configure.


### User Experience

- A concurrency limit will be applied to request deocding to ensure that TCP-based sources propagate backpressure appropriately, and don't consume too much memory.

### Implementation

- Explain your change as if you were presenting it to the Vector team.
- When possible, demonstrate with pseudo code not text.
- Be specific. Be opinionated. Avoid ambiguity.


## Prior Art

- List prior art, the good and bad.
- Why can't we simply use or copy them?

## Alternatives

- What other approaches have been considered and why did you not choose them?
- How about not doing this at all?

## Outstanding Questions

- List any remaining questions.
- Use this to resolve ambiguity and collaborate with your team during the RFC process.
- *These must be resolved before the RFC can be merged.*

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change.
- [ ] Incremental change #1
- [ ] Incremental change #2
- [ ] ...

Note: This can be filled out during the review process.

## Future Improvements

- List any future improvements. Use this to keep your "plan of attack" scope small and project a sound design.
