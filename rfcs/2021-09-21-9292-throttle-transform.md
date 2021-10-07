# RFC 9292 - 2021-09-21 - `throttle` transform

This RFC proposes the addition of a new transform that provides a user the ability to control the throughput of specific event streams.

## Context

* [`throttler` transform](https://github.com/vectordotdev/vector/issues/258)
* [Dead letter queues](https://github.com/vectordotdev/vector/issues/1772)

## Scope

### In scope

* Dropping logs to rate limit an event stream
* Rate limit by both bytes (unserialized) and events
* Exclude events with VRL conditions
* Optionally specify buckets based on the value of a key in the event

### Out of scope

* Apply backpressure as a form of rate limiting

## Pain

* Users lack the necessary tooling to control throughput which can cause excessive costs and negatively impact downstream services due to increased load
* Admins cannot set quotas for users/usergroups utilizing Vector

## Proposal

### User Experience

The `throttle` transform can be used to rate limit specific subsets of your event stream to limit load on downstream services or to enforce quotas on users.
You can enforce rate limits on events or their raw size, as well as excluding events based on a VRL condition to avoid dropping critical logs. Rate limits
can be applied globally across all logs or by specifying a key to create buckets of events to rate limit more granularly.

The initial implementation will shed load by dropping any events beyond the configured rate limit. This could be configured in the future to route events to
a dead letter queue, or possibly apply backpressure to upstream sources.

### Implementation

The `throttle` transform will leverage the existing [Governor](https://docs.rs/governor/0.3.2/governor/index.html) crate. We will expose the bytes or events
per second as user-facing configuration (or expand to more time frames) and translate that into a `governor::Quota` for internal use. We should not expose
the `Quota` directly to users to ensure ease of configuration for end users.

Config:

```rust
pub struct ThrottleConfig {
    threshold: Threshold,
    window: f64,
    key_field: Option<String>,
    exclude: Option<AnyCondition>,
}

struct Threshold {
    events: u32,
    bytes: u32,
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
	let quota = Quota::with_period(Duration::from_secs(self.window))
            .unwrap()
	    .allow_burst(self.threshold);
        let lim = RateLimiter::keyed(quota);

	let mut flush_keys = tokio::time::interval(Duration::from_secs(self.window * 2);
        let mut flush_stream = tokio::time::interval(Duration::from_millis(1000));

        Box::pin(
            stream! {
              loop {
                let mut output = Vec::new();
                let done = tokio::select! {
                    _ = flush_stream.tick() => {
                        false
                    }
		    _ = flush_keys.tick() => {
			lim.retain_recent();
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
                                        emit!(EventRateLimited);
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

* Controlling throughput provides better reliability for downstream services which can be critical in observability platforms
* Setting granular quotas is invaluable as an administrator running Vector for a number of users to ensure fair use of the pipeline

## Drawbacks

* When rate limiting by `Bytes` in a transform it will be the unserialized form of the event, which will differ from what downstream sinks will actually receive

## Prior Art

* [Governor](https://docs.rs/governor/0.3.2/governor/index.html)
* [Throttle - FluentBit](https://docs.fluentbit.io/manual/pipeline/filters/throttle)
* [grouper::bucker - Tremor](https://github.com/tremor-rs/tremor-runtime/blob/main/tremor-pipeline/src/op/grouper/bucket.rs)

## Alternatives

* Extend `sample` transform to allow for a window configuration
* Build rate limiting controls as a generic `sink` feature, rather than a separate `transform`

## Outstanding Questions

* ~~Rate limiting seems like it could be generically a `sink` concern and implemented as a composable part of our `sink` pattern. This could give more "accurate" serialized sizes and possibly be easier to manage for administrators (depending on needs). If rate limiting is also a `sink` concern should it only be implemented there or also available as a `transform`?~~ We may additionally add this to sinks, but this transform has value on its own.

## Plan Of Attack

* [ ] [feat(new transform): Initial throttle transform spike](https://github.com/vectordotdev/vector/pull/9378)
* [ ] Add documentations

## Future Improvements

* Configure the transform with additional behaviors
* Batching multiple events through the rate limiter
* Optionally persist rate limiter state across restarts
