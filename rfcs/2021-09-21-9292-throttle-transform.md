# RFC 9292 - 2021-09-21 - `throttle` transform

This RFC proposes the addition of a new transform that provides a user the ability to control the throughput of specific event streams.

## Context

- [#258](https://github.com/vectordotdev/vector/issues/258)

## Scope

### In scope

- Dropping events (logs and metrics) to rate limit an event stream
- Rate limit by both bytes (unserialized) and events
- Exclude events with VRL conditions
- Specify buckets based on the value of a key in the event

### Out of scope

- Apply backpressure as a form of rate limiting

## Pain

- Users lack the necessary tooling to control throughput which can cause excessive costs and negatively impact downstream services due to increased load
- Admins cannot set quotas for users/usergroups utilizing Vector

## Proposal

### User Experience

The `throttle` transform can be used to rate limit specific subsets of your event stream to limit load on downstream services or to enforce quotas on users.
You can enforce rate limits on events or their raw size, as well as excluding events based on a VRL condition to avoid dropping critical logs. Rate limits
can be applied globally across all logs or by specifying a key to create buckets of events to rate limit more granularly.

### Implementation

The `throttle` transform will leverage the existing [Governor](https://docs.rs/governor/0.3.2/governor/index.html) crate. We will expose the bytes or events
per second as using facing configuration (or expand to more time frames) and translate that into a `governor::Quota` for internal use. We should not expose
the `Quota` directly to users to ensure ease of configuration for end users.

Config:

```rust
pub struct ThrottleConfig {
    events_per_second: u32,
    bytes_per_second: u32,
    key_field: Option<String>,
    exclude: Option<AnyCondition>,
}
```

TaskTransform:

```rust
impl TaskTransform for Throttle {
    fn transform(
        self: Box<Self>,
        mut input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let lim = RateLimiter::keyed(Quota::per_second(self.events_per_second));

        let mut flush_stream = tokio::time::interval(Duration::from_millis(1000));

        Box::pin(
            stream! {
              loop {
                let mut output = Vec::new();
                let done = tokio::select! {
                    _ = flush_stream.tick() => {
                        false
                    }
                    maybe_event = input_rx.next() => {
                        match maybe_event {
                            None => true,
                            Some(event) => {
			    	if let Some(condition) = self.exclude_as_ref() {
				    if condition.check(&event) {
				        output.push(event);
					false
				    }
				}

				let value = self
				    .key_field
				    .as_ref()
				    .and_then(|key_field| event.get(key_field))
				    .map(|v| v.to_string_lossy());

                                match lim.check_key_n(value, size) {
                                    Ok(()) => {
                                        output.push(event);
                                        false
                                    }
                                    _ => {
                                        // Dropping event
                                        false
                                    }
                                }
                            }
                        }
                    }
                };
                yield stream::iter(output.into_iter());
                if done { break }
              }
            }
            .flatten(),
        )
    }
}
```

## Rationale

- Controlling throughput provides better reliability for downstream services which can be critical in observability platforms
- Setting granular quotas is invaluable as an administrator running Vector for a number of users to ensure fair use of the pipeline

## Drawbacks

- When rate limiting by `Bytes` in a transform it will be the unserialized form of the event, which will differ from what downstream sinks will actually receive

## Prior Art

- [Governor](https://docs.rs/governor/0.3.2/governor/index.html)
- [Throttle - FluentBit](https://docs.fluentbit.io/manual/pipeline/filters/throttle)
- [grouper::bucker - Tremor](https://github.com/tremor-rs/tremor-runtime/blob/main/tremor-pipeline/src/op/grouper/bucket.rs)

## Alternatives

- Extend `sample` transform to allow for a window configuration
- Build rate limiting controls as a generic `sink` feature, rather than a separate `transform`

## Outstanding Questions

- Rate limiting seems like it could be generically a `sink` concern and implemented as a composable part of our `sink` pattern. This could give more "accurate" serialized sizes and possibly be easier to manage for administrators (depending on needs). If rate limiting is also a `sink` concern should it only be implemented there or also available as a `transform`?

## Plan Of Attack

- [ ] [feat(new transform): Initial throttle transform spike](https://github.com/vectordotdev/vector/pull/9378)
- [ ] ...

## Future Improvements

- Throttle by applying backpressure rather than dropping events completely
- Batching multiple events through the rate limiter
- ...
