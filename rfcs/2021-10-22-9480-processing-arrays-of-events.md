# RFC 9480 - 2021-10-18 - Processing Arrays of Events

The primary unit of data transfer between components in a pipeline is
currently a single event. This has significant performance costs,
particularly for those components that handle multiple events at a
time. It also puts a limit on the maximum performance possible due to
overheads in the communication primitives such as lock contention. In
order to remove these limiting factors, Vector should be modified to
allow for processing multiple events at once.

## Cross cutting concerns

All consuming components (transforms and sinks) need to handle log and
metric events through separate code paths. A change to the core `Event`
data type provides the opportunity to rethink how these might be made
more generic.

## Scope

### In scope

- Data structures related to arrays of events.

- Changes to components to enable processing arrays of events.

### Out of scope

This proposal may unlock future optimizations because of the new data
structures, such as parallelizing transforms with `rayon`. This is an
important consideration, but not a driving feature of this proposal.

- Future optimizations that may be unlocked by the proposed array data
  structures, such as parallelizing transforms with `rayon`.

- The addition of new variants to the base `Event` type to support plans
  for new data types such as traces.

- The addition of traits or other support to allow components to be more
  generic over different types of events.

## Proposal

Vector should introduce a new data type to facilitate transporting
arrays of events between components, and the necessary traits to make
components work generically over either single events or arrays. It is
critical that all enhancements avoid requiring expensive rewrites across
all components.

### User Experience

This change should be completely invisible to the user, other than
performance changes.

### Implementation

#### Introducing a new type for arrays of events

The simplest way to represent an arbitrary sized array of events is the
built-in `Vec` type. However, in the short-term, most producing
components (sources and transforms) will only be outputting a single
event at a time. As such, it is worth optimizing the data type for a
single event being a common case. This is an ideal use of the `SmallVec`
type, which can store a fixed number of elements inline, or switch to a
standard `Vec` when that overflows.

Additionally, since sources will produce arrays of exclusively logs or
metrics, and consuming components need to detect what type of data is
contained in the array, this type is required to provide that
information up-front in a similar manner the base `Event` type does.

```rust
pub enum EventVec {
    Logs(SmallVec<[LogEvent; 1]>),
    Metrics(SmallVec<[Metric; 1]>),
}
```

#### Generic event container trait

Several of the components will need to be generic over what type of data
they are handling, either a single `Event` or an array. This can be
simply modelled as an iterator using existing traits. We can add
additional methods to this trait later as needed to support needs beyond
simple iteration (ie batching in sinks).

```rust
trait EventContainer: ByteSizeOf {
    type EventIter;
    type LogIter;
    type MetricIter;
    fn into_events(self) -> Self::EventIter;
    fn into_logs(self) -> Self::LogIter;
    fn into_metrics(self) -> Self::MetricIter;
}

impl IntoIterator for EventVec { … }
impl IntoIterator for Event { … }
```

#### Make `Pipeline` accept Enhancing the `Pipeline`

The `Pipeline` structure stands as the primary unit for moving data
between components in Vector. It receives from a source or transform,
passes through optional inline transforms, and then sends the result
one-by-one to a receiving transform or sink. However, before sending to
the receiver, these events are pushed into a queue.

In order to receive events asynchronously from sources, the pipeline
implements the `Sink` trait for single events. This trait is
parameterized over the type which the pipeline can receive. The current
implementation allows for sending a single `Event`, and an additional
implementation can be added for arrays of `Event`.

```rust
impl Sink<EventVec> for Pipeline { … }
```

#### Split the `Pipeline` implementation

#### Introduce transform variants for arrays

There currently exist three types of transforms, expressed as traits:

1. `FunctionTransform` receives a single event and outputs into a
   mutable vector of events.
2. `FallibleFunctionTransform` is similar, but has separate outputs for
   successful and failed events.
3. `TaskTransform` runs as an asynchronous task, accepting a stream of
   events and outputting a stream of events.

These will need to be rewritten to accept an event container as their
input, and as the output for the task. While most transforms can be
easily rewritten to handle the iterator provided by the container,
they will initially simply be handled by a wrapper that will convert the
container, iterate over the events individually, and collect the result.

```rust
trait FunctionTransform<T: EventContainer>: Send + dyn_clone::DynClone + Sync {
    fn transform(&mut self, output: &mut Vec<T>, event: T);
}

trait FallibleFunctionTransform<T: EventContainer>: Send + dyn_clone::DynClone + Sync {
    fn transform(&mut self, output: &mut Vec<T>, errors: &mut Vec<T>, event: T);
}

trait TaskTransform<T: EventContainer>: Send {
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn Stream<Item = T> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = T> + Send>>
    where
        Self: 'static;
}

// Allow existing transforms to be converted with `.into()`

impl<T: FunctionTransform<Event>> From<T> for Transform { … }

impl<T: FallibleFunctionTransform<Event>> From<T> for Transform { … }

impl<T: TaskTransform<Event>> From<T> for Transform { … }
```

#### Sinks

There are two kinds of sinks in Vector:

1. Push-style which accept individual events through the `Sink` trait
   function `start_send`.
2. Pull-style `StreamSink` which runs an async task that fetches
   individual events from its input `Stream`.

These will be rewritten to accept an array of events at a time instead
of a single event.  Conversions for each of these will be provided to
allow existing code to be ported with only the addition of a `.into()`
in the `build` function.

```rust
enum VectorSink {
    Sink(Box<dyn Sink<EventVec, Error = ()> + Send + Unpin>),
    Stream(Box<dyn StreamSink<EventVec> + Send>),
}

trait StreamSink<T: EventContainer> {
    async fn run(self: Box<Self>, input: BoxStream<'_, T>) -> Result<(), ()>;
}

// Allow existing sinks be converted with `.into()`

impl<T: Sink<Event, Error = ()>> From<T> For VectorSink { … }

impl<T: StreamSink<Event> + Send> From<T> for VectorSink { … }
```

## Rationale

- Why is this change worth it?
- What is the impact of not doing this?
- How does this position us for success in the future?

## Drawbacks

- Why should we not do this?
- What kind on ongoing burden does this place on the team?

## Prior Art

- List prior art, the good and bad.
- Why can't we simply use or copy them?

## Alternatives

The most obvious representation for an array of events would be `Vec`
directly. However, most components will continue to communicate using
individual events, at least initially. The `SmallVec` type allows for
sending those single events as efficiently as we did previously, while
switching to a `Vec` when more than one is in the array.

Additionally, having an array of the `Event` type means that consuming
components are required to switch on each event emitted from the
iteration, thus preventing any real optimizations in transforms and
sinks.

## Outstanding Questions

- List any remaining questions.
- Use this to resolve ambiguity and collaborate with your team during the RFC process.
- *These must be resolved before the RFC can be merged.*

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Implement `Sink<EventVec> for Pipeline`.
- [ ] Rewrite sink types to accept `EventVec` through wrapper functions.
- [ ] Rewrite transform types to accept `EventVec` through wrapper functions.
- [ ] Modify `Pipeline` to send `EventVec` to its receiver.
- [ ] Convert sources that can send arrays of events to send `EventVec`.
- [ ] Convert transforms that can easily process arrays of events to consume `EventContainer`.
- [ ] Convert sinks that can process arrays of events to consume `EventContainer`.

## Future Improvements

Since producing components will tend to produce events with a similar
structure in each output, a potential optimization to the internal
storage of arrays of events is to have the structure represented once
along with arrays of values. This could be done in two different ways,
both of which have challenges for actually manipulating the data:

1. Replace all leaf values with arrays of scalar values. In this format,
   an event would be composed of many arrays.
2. Flatten out the structure into an array of keys and replace the leaf
   values with an indexed array of arrays.
