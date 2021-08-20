# RFC #2768 - 2020-06-12 - Batch and Buffer Rework

## Motivation

Most sinks in Vector batch up events to send in a buffer in order to
improve efficiency and increase transmission rates. However, the current
implementations of batching suffer from two significant problems:

1. Batching sinks allow for configuring only the maximum number of
   events or the maximum size of the batch (in bytes) but not
   both. Worse, this is hard-coded into the sink, so that the choice of
   which one is allowed is made by Vector.
2. For sinks that limit batch sizes in bytes, a buffer is considered
   ready to send when the size reaches *or exceeds* the given size
   limit. If the size limit is close to the maximum allow request size
   for a remote service, this will routinely lead to problems when the
   flow rate is high enough to avoid triggering a batch timeout.

## Guide-Level Proposal

For sinks that use the `BatchSink` framework (`PartitionBatchSink` is
similar), events move along the following path:

1. Each event is optionally encoded into a more suitable format for
   buffering (using `with_flat_map` on the batch sink, ref `type
   Batch::Input`).
2. The encoded item is pushed into a buffer (in
   `BatchSink::start_send`).
3. When that buffer is full, as determined by being at or above the
   configured batch size, it is collected into a batch (in
   `BatchSink::poll_complete`).
4. The batch is turned into a request (in `trait Service::call`) which
   will do the final encoding and serialization.

There exist the following buffer types:

- `Buffer` offers (gzip) compression, but requires the events be
  serialized before insertion and that serialization must include a
  terminator. Events are stored and output as one large `Vec<u8>`, which
  allows for easy computation of both sizes.
- `JsonArrayBuffer` internally serializes events into "raw" JSON, which
  may be then added to another structure after output. Events are stored
  as `serde_json::value::RawValue` which is a newtype over a boxed
  string, which allows for maintaining a running total of both sizes.
- `MetricBuffer` stores metrics in their structured form, without
  encoding. Since the target serialization is unknown, this buffer
  cannot know the total byte size.
- `PartitionBuffer<T, K>` is a meta-buffer, that partitions events into
  multiple buffers of type `T`.
- `Vec<T>` can only count events, not bytes, as it is agnostic to the
  size of `T`.

We want to get to the point where as many batching sinks as possible can
be configured with either a byte limit or an event limit, or
both. Additionally, the batch limits must be inviolable, so that no
requests are ever sent that exceed size limits. To accomplish this the
following is necessary:

### Unified Batch Configuration

The split configuration types for bytes and events will be rolled into a
single configuration combining both attributes. Batch sizes will be
limited to which ever maximum is reached first.

The `max_size` element is retained for backwards compatibility, and each
sink will use it as a default for the maximum bytes or events depending
on the previous configuration mode. If both `max_size` and either of
`max_bytes` or `max_events` are set, Vector will produce a configuration
error indicating a conflict in the configuration.

```rust
struct BatchConfig {
    max_bytes: Option<usize>,
    max_events: Option<usize>,
    max_size: Option<usize>,
    timeout_secs: Option<u64>,
}
```

### Failable Batch Insertion

The current batch buffers always allow events to be inserted, and then
check separately if the buffer is "big enough" to be sent. This will be
changed into a new signature that indicates if the insertion would push
the buffer over its size limits.

```rust
#[must_use]
enum PushResult<E> {
    Ok,
    Overflow(E),
}

trait Batch {
    fn push(&mut self, event: Self::Input) -> PushResult<Self::Input>;
}
```

This will require all the buffers to know their maximum size. This will
not be encoded into the trait directly, as each buffer will implement
their own creation method to accommodate additional creation parameters,
such as the compression mode or partition key.

## Outstanding Questions

Does it make sense to move conversion of input events into the
intermediate `type Input` into the buffer types instead of using a map
on the sink? This would mean something like a `TryInto` trait,
implemented differently for each sink, or parameterizing the buffer with
something the way `RetryLogic` works for the sink trait.

Similarly, does it make sense to move serialization of output events
into the trait? Is this possible for `MetricBuffer`? If this could be
done for all buffers, it would guarantee availability of the byte size
limit across the board.

## Implementation Plan

The desired goal can be accomplished incrementally as follows:

1. Enhance all the buffers to store their maximum size. This will
   require replacing the plain `Vec<T>` buffer with a new `VecBuffer<T>`
   type.
2. Move the "is full" logic from `BatchSink` into `trait Batch`.
3. Modify `Batch::push` to return an indicator that the item would
   overflow the buffer.
4. Modify the sending logic in `impl Sink for BatchSink` to use failure
   from the new push method to cause a new request to be generated,
   making `start_send` exit without consuming the new event.
5. Merge the separate `BatchBytesConfig` and `BatchEventsConfig` into a
   unified `BatchConfig`. Buffers that do not support byte size limits
   (like `MetricBuffer`) would error on creation if they were configured
   with one.
