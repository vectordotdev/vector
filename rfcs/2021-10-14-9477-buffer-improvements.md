# RFC 9477 - 2021-10-14 - Buffer Improvements

Vector currently provides two simplistic modes of buffering -- the storing of events as they move
_between_ components -- depending on the reliability requirements of the user.  Over time, many
subtle issues with performance and correctness have been discovered with these buffer
implementations. As well, users have asked for more exotic buffering solutions that better fit their
architecture and operational processes.

As such, this RFC lays out the groundwork for charting a path to improving the performance and
reliability of Vector’s buffering while simultaneously supporting more advanced use cases that have
been requested by our users.

## Context

- **Document how Vector's buffers work better when multiple sinks are used**
  [#4455](https://github.com/vectordotdev/vector/issues/4455)
- **Optimize disk buffers** [#6512](https://github.com/vectordotdev/vector/issues/6512)
- **External buffer support (kafka, kinesis, etc)**
  [#5463](https://github.com/vectordotdev/vector/issues/5463)
- **Layered / waterfall / overflow buffer support**
  [#5462](https://github.com/vectordotdev/vector/issues/5462)
- **Improve vector start-up times when there is a large disk buffer**
  [#7380](https://github.com/vectordotdev/vector/issues/7380)

## Cross-cutting concerns

- The effect of buffering on end-to-end acknowledgment:
  [#7065](https://github.com/vectordotdev/vector/issues/7065)

## Scope

### In scope

- Fixing the performance and reliability issues of the existing in-memory and disk-backed buffer
  strategies.
- Providing new/alternative buffer strategies that users have asked us for.
- Buffer flexibility with regards to fan-out processing and disaster recovery.

### Out of scope

- Changes to disk buffering that are specific to certain types of disks / filesystems.

## Pain

Vector users often switch to disk buffering to provide a level of reliability in the case that
downstream sinks experience temporary issues, or in the case that the machine running Vector, or
Vector itself, have problems that could cause it to crash.

Effectively, these users turn to disk buffering to increase reliability of their Vector-based
observability pipelines.  However, while disk buffering can help if Vector encounters issues, it
does not always help if the disk itself or the machine itself experience problems.

Likewise, we currently push all events through disk buffers when they are enabled, which introduces
a performance penalty.  Even if the source and sink could talk to each other without any
bottlenecks, users pay the cost of writing every event to LevelDB, and then reading that event back
from LevelDB.  It may or may not be written to disk at that point, which means sometimes writes and
reads are fast and sometimes, when they have to go to disk, they’re not, which introduces further
performance variability.

## Proposal

### User Experience

Buffering would be separated and documented as consisting of multiple facets:

- **Buffer type:** _in-memory_ vs _disk_ vs _external_ (and the pros/cons for each)
- **Buffer mode:** _block_ vs _drop newest_ vs _overflow_

If external buffering is utilized, multiple Vector processes with an identical configuration can
participate in processing the events stored in the external buffer.  Crucially, the newest feature
would be configuring a buffer in “overflow” mode, which would configure an in-memory channel that,
when facing backpressure, writes to either a disk or external buffer.

We would maintain on-the-wire compatibility with the Vector source/sink by using Protocol Buffers,
as we do today for disk buffering.  This would allow us to maintain a common format for events that
would be usable even between different versions of Vector.

### Implementation

- Rewrite disk buffers, switching to an append-only log format.
  - Primarily, no more LevelDB.  No more C/C++ libraries.  All Rust code.  Code that we control.
  - Conceptually, this would look a lot like the writer side simply writing lines to a file, and
    the reader side tailing that file.
  - We would end up having some small metadata on disk that the writer used to keep track of log
    files, and the reader would similarly have some small metadata on disk to track its read
    progress.
  - Records would be checksummed on disk in order to detect corruption.  We would use a CRC32
    checksum at the end of each record for speed and reasonable resiliency.
    ([#8671](https://github.com/vectordotdev/vector/issues/8671))
  - For disk buffers, fsync behavior would be configurable, allowing users to choose how often
    (time) the buffer should be synchronized to disk.
    ([#8540](https://github.com/vectordotdev/vector/issues/8540))
- In-memory buffering would simply stay as it exists now, as a raw channel.
- Disk buffering and external buffers would become independent reader and writer Tokio tasks that
  are communicated with via channels.
- External buffers would be implemented with as little of a shim layer as possible.  While
  functioning like normal topology components, we don’t want to share code between them and existing
  sources/sinks.
- Buffers as a whole would be tweaked to become composable in the same style as `tower::Service`:
  - All buffers would represent themselves via simple MPSC channels.
  - Two new types, for sending and receiving, would be created as the public-facing side of a
    buffer.  These types would provide a `Sink` and `Stream` implementation, respectively:

    ```rust
    struct BufferSender {
      base: PollSender,
      base_ready: bool,
      overflow: Option<BufferSender>,
      overflow_ready: bool,
    }

    struct BufferReceiver {
      base: PollReceiver,
      overflow: Option<BufferReceiver>,
    }
    ```

  - The buffer wrapper types would coordinate how the internal buffer channels are used, such that
    they were used in a cascading fashion:

      ```rust
      impl Sink<Event> for BufferSender {
        fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
          // Figure out if our base sender is ready, and if not, and we have an overflow sender configured, see if _they're_ ready.
          match self.base.poll_ready(cx) {
            Poll::Ready(Ok(())) => self.base_ready = true,
            _ => if let Some(overflow) = self.overflow {
              if let Poll::Ready(Ok(())) = overflow.poll_ready(cx) {
                self.overflow_ready = true;
              }
            }
          }

          // Some logic here to handle dropping the event or blocking.

          // Either our base sender or overflow is ready, so can we proceed.
          if self.base_ready || self.overflow_ready {
            Poll::Ready(Ok(()))
          } else {
            Poll::Pending
          }
        }

        fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
          let result = if self.base_ready {
            self.base.start_send(item)
          } else if self.overflow_ready {
            match self.overflow {
              Some(overflow) => overflow.start_send(item),
              None => Err("overflow ready but no overflow configured")
            }
          } else {
            Err("called start_send without ready")
          };

          self.base_ready = false;
          self.overflow_ready = false;
          result
        }
      }
      ```

  - Thus, `BufferSender` and `BufferReceiver` would contain a "base" sender/receiver that
      represents the "primary" buffer channel for that instance, with the ability to "overflow" to
      another instance of `BufferSender`/`BufferReceiver`.  While `BufferSender` would try the base
      sender first, and then the overflow sender, `BufferReceiver` would operate in reverse, trying
      the overflow receiver first, then the base receiver
  - This design isn’t necessarily novel, but if designed right, allows us to arbitrarily augment
      the in-memory channel with an overflow strategy, potentially nested even further: in-memory
      overflowing to external overflowing to disk, etc.
  - To tie back it back to `tower::Service`, in this sense, all implementations would share a common
    interface that allows them to be layered/wrapped in a generic way.
- Augment the buffer configuration logic/types such that multiple buffers can be defined for a
  single sink, with enough logic so that we can parse both the old-style single buffer and the
  new-style chained buffers.

## Rationale

Vector’s raison d'être is that it provides both reliability **and** performance compared to other
observability solutions.  Vector’s current capabilities for handling errors -- regardless of whether
they’re in Vector itself or at a lower level like the operating system or hardware -- do not meet
the bar of being both reliable and performant.  To that end, improving buffers is simply table
stakes for meeting our reliability and performance goals.

If we did not do this, we could certainly still meet our performance goals, but users would not be
able to confidently use Vector in scenarios that required high reliability around collecting and
shipping observability events.  We already know empirically that many users are waiting for these
types of improvements before they will consider deploying, and depending on, Vector in their
production environments.

## Drawbacks

Practically speaking, buffering always imposes a performance overhead, even if minimal.  Disks can
have inexplicably slow performance.  Talking to external services over the network can encounter
transient but harmful latency spikes.  While we will be increasing the reliability of buffering,
we’ll also be introducing another potential source of unexpected latency in the overall processing
of events.

Of course, we can instrument this new code before-the-fact, and attempt to make sure we have
sufficient telemetry to diagnose problems if and when they occur, we’re simply branching out into an
area that we don’t know well yet, and there’s likely to be a learning curve spent debugging any
potential issues in the future as this code is put through its paces in customer environments.

## Prior Art

Many of the alternatives to Vector offer some form of what we call buffering:

- Tremor offers a WAL, or write-ahead log, operator that serializes all events in a pipeline through
  a write-then-read on disk, similar to the current disk buffering behavior of Vector.
- Cribl offers disk-backed overflow buffering, called Persistent Queues, which only sends events to
  disk when its in-memory queues have reached their capacity.
- Fluentd offers a hybrid write-ahead/batching approach, where events can be written to disk first,
  and then flushed from disk on a configurable interval.

Generally speaking, I believe our intended approach is the best possible summation of the various
approaches taken by other alternative projects, and there is no specific reason to directly copy or
tweak our approach based on how they have done it.

In terms of the implementation of the RFC, there exists only one crate that is fairly close to our
desire for a disk-based reader/writer channel, and that is hopper. However, hopper itself is not a
direct fit for our use case:

- it does not have an asynchronous API, which would lead us back to the same design that we
  currently use with LevelDB
- it implements an in-memory channel w/ disk overflow as the base, which means that we would have to
  run two separate implementations: hopper for in-memory or in-memory w/ disk overflow, and then a
  custom one for in-memory w/ external overflow

Due to this, it is likely that we look at hopper simply as a guide of how to structure and rewrite
the disk buffer, as well as for ideas on how to test it and validate its performance and
reliability.

## Alternatives

The simplest alternative approach overall would be users running Vector such that their
observability events got written to external storage: this could be anything from Kafka to an object
store like S3.  They would then run another Vector process, or separate pipeline in the same Vector
process, that read those events back.  Utilizing end-to-end acknowledgements, they could be sure
that events were written to the external storage, and then upon processing those events on the other
side, also using end-to-end acknowledgements, they could be sure they reached the destination.

The primary drawback with this approach is that it requires writing all events to external storage,
rather than just ones that have overflowed, which significantly affects performance and increases
cost.

## Outstanding Questions

- [x] Should we prioritize a particular external buffer implementation first?  For example, Kafka vs
  SQS, or S3 vs GCP.
  - **Answer:** We'll start with implementing S3 as the first external buffer type.
- [x] Should we reconsider using the existing sinks/sources to power external buffers?
  - Supporting batching of values to make it more efficient, handling service-specific
      authentication, etc, would be trivial if we used the existing code.
  - It may also inextricably tie us to code that is hard to test and hard to document in terms of
      invariants and behavior.
  - **Answer:** No, we will stick with the isolated code design/approach.
- [x] Is it actually possible for us to (de)serialize the buffer configuration in a way that we can
  detect both modes without overlap?
  - **Answer:** We should be able to wrap the existing buffer configuration type with an enum, and
    add another variant to support an "extended" configuration.  We already have examples of
    `serde::Serializer` implementations that can generate varying outputs based on the value of a
    given field, so we will add a new field, tentatively called `advanced`, that can be set to
    `true` to unlock support for defining a series of buffer types that get nested within one another.
- [x] Can we reasonably support “drop oldest” behavior? This would mean having to hijack the reader side
  to forcefully advance it, adding an extra constraint around the design and behavior of each
  buffer.  For in-memory only, it could even imply a lock around the receiver itself.
  [#8209](https://github.com/vectordotdev/vector/issues/8209)
  - **Answer:** I do not believe we can reasonably support this without hamstringing our performance
    for all buffer types.  While some users may have asked for it, it does not seem to have enough
    traction to warrant focusing energy on coming up with a performant design that supports it.

## Plan Of Attack

- [ ] Create the two new sender/receiver types that can be generalized over whatever the underlying
  buffer implementation is.  These types would be where the “block vs drop vs overflow” behavior
  would live.
- [ ] Create a new buffer builder that would allow us to do the aforementioned chained buffering.
- [ ] Refactor the buffer configuration code so that it can be parsed both in the old and new style.
- [ ] Rewrite the existing disk buffer implementation to the new append-only design.
- [ ] Rewrite the existing in-memory implementations so it can be wrapped by the aforementioned
  sender/receiver types.
- [ ] Update the topology builder code to understand and use the new-style buffer configuration.
- [ ] Write an external buffer implementation for Kafka.

## Future Improvements

- Shared per-process limits on disk usage or size of the external buffers.  Currently, buffers
  operate independently from one another, but this could lead to buffers that, for example, fight
  for disk space. [#5102](https://github.com/vectordotdev/vector/issues/5102)
- Make sure we gracefully handle disk out of space errors.
  [#8763](https://github.com/vectordotdev/vector/issues/8763)
- Using `io_uring`  on Linux (likely via `tokio-uring`) to drive the disk buffer I/O for lower overhead,
  higher efficiency, higher performance, etc.  Potentially usable for Windows as well if
  `tokio-uring` gained support for Windows' new I/O Rings feature.
