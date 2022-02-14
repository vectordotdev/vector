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

- Future optimizations that may be unlocked by the proposed array data
  structures, such as parallelizing transforms with `rayon`.

- The addition of new variants to the `Event` type to support plans for
  new data types such as traces.

- The addition of traits or other support to allow components to be more
  generic over different types of events.

- Replacing the core `enum Event` type with the contained `LogEvent` and
  `Metric` to be able to encode which variant a component accepts in the
  type system.

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

#### Introducing a new types for arrays of events

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

The internal types will are explicitly named to allow components to use
the internal types without needing to know the implementation details.

```rust
pub type LogVec = SmallVec<[LogEvent; 1]>;
pub type MetricVec = SmallVec<[Metric; 1]>;

pub enum EventVec {
    Logs(LogVec),
    Metrics(MetricVec),
}
```

#### Generic container traits

Several of the components will need to be generic over what type of data
they are handling, either a single `Event` or an array. This can be
simply modelled as an iterator using existing traits. We can add
additional methods to this trait later as needed to support needs beyond
simple iteration (ie batching in sinks).

As above, this is broken down into separate container traits for each
internal variant and the containing event enum. All implementations are
expected to be trivial wrappers around existing iterators.

```rust
trait EventContainer: ByteSizeOf {
    type Iter: Iterator<Item = Event>;
    fn into_events(self) -> Self::Iter;
}

impl EventContainer for Event { … }
impl EventContainer for EventVec { … }
impl EventContainer for LogEvent { … }
impl EventContainer for LogVec { … }
impl EventContainer for Metric { … }
impl EventContainer for MetricVec { … }

trait LogContainer: ByteSizeOf {
    type Iter;
    fn into_logs(self) -> Self::Iter;
}

impl LogContainer for LogEvent { … }
impl LogContainer for LogVec { … }

trait MetricContainer: ByteSizeOf {
    type Iter;
    fn into_metrics(self) -> Self::Iter;
}

impl MetricContainer for Metric { … }
impl MetricContainer for MetricVec { … }
```

#### Enhancing the `Pipeline`

The `Pipeline` structure stands as the primary unit for moving data
between components in Vector. It receives from a source or transform,
passes through optional inline transforms, and then sends the result
one-by-one to a receiving transform or sink. However, before sending to
the receiver, these events are pushed into a queue.

In order to receive events asynchronously from sources, the pipeline
implements the `Sink` trait for single events. This trait is
parameterized over the type which the pipeline can receive. The current
implementation allows for sending a single `Event`, and an additional
implementation can be added for event arrays. This allows sources to be
converted individually to send arrays while retaining compatibility with
existing sources.

```rust
impl Sink<EventVec> for Pipeline { … }
```

`Pipeline` internally stores a dequeue of `Event`. In order for it to
handle arrays of events, the simplest conversion would be to have a
similar deque of `EventVec`. In this form, each send into the pipeline
will simply push a new array onto the dequeue.

With this form in place, a useful optimization is possible. Since most
sources only send a single event type into the pipeline, we can extend
the last item on the current queue with new items if the variants
match. This allows us to collect events from sources that emit a single
event at a time into arrays that can be forwarded to consumers.

```rust
pub struct Pipeline {
    inner: mpsc::Sender<Event>,
    // We really just keep this around in case we need to rebuild.
    #[derivative(Debug = "ignore")]
    inlines: Vec<Box<dyn FunctionTransform>>,
    enqueued: VecDeque<EventVec>,
}
```

#### Arrays of events in transforms

There currently exist three types of transforms, expressed as traits:

1. `FunctionTransform` receives a single event and outputs into a
   mutable vector of events.
2. `FallibleFunctionTransform` is similar, but has separate outputs for
   successful and failed events.
3. `TaskTransform` runs as an asynchronous task, accepting a stream of
   events and outputting a stream of events.

The two function transforms are turned into tasks within the topology
builder code. This task uses the `ready_chunks` stream adapter to pulls
arrays out of the input component(s) and then processes them in a loop
before forwarding them to the consumer components. This process can
easily be adapted to pull in and loop over the arrays of events
described above. In the same manner as `ready_chunks`, this process can
be further optimized to merge arrays of the same type of event when
multiples are ready at the same time.

The task transform, however, will need to be rewritten to accept an
event container as its input, and as the output for the task. While most
such transforms can be easily rewritten to handle the iterator provided
by the container, they will initially simply be handled by a wrapper
task that will iterate over the events in the container individually,
and collect the result before forwarding it into the output stream.

```rust
trait TaskTransform<T: EventContainer>: Send {
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn Stream<Item = T> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = T> + Send>>
    where
        Self: 'static;
}

// Allow existing task transforms to be converted with `.into()`
impl<T: TaskTransform<Event>> From<T> for Transform { … }
```

#### Arrays of events in sinks

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

The primary rationale for these changes stems straight from the
motivation: performance. By working on arrays of events at a time, we
reduce the per-event overhead of all processing steps, improving our
margins of performance with minimal code changes. Further, it unlocks
options for further optimizations down the road that are only possible
when working on arrays.

By introducing a trait to represent a container of events, it will make
experiments with alternate representations easier once the trait is
fully utilized.

## Drawbacks

This change necessarily moves the complexity of dealing with arrays of
events into all consuming components. Even if no such component is
modified beyond the trivial wrapper functions, this will require a
growth in the code required to consume events, albeit small.

Additionally, there may be some memory effects caused by moving events
to and from a heap-allocated vector. It is likely those effects will be
negligible if present, but that cannot be determined without before
making the changes.

## Alternatives

The most obvious representation for an array of events would be `Vec`
directly. However, most components will continue to communicate using
individual events, at least initially. The `SmallVec` type allows for
sending those single events as efficiently as we did previously, while
switching to a `Vec` when more than one is in the array.

Additionally, having an array of the `Event` type directly means that
consuming components are required to switch on each event emitted from
the iteration, thus preventing any real optimizations in transforms and
sinks.

Given the event container traits, the pipeline could be made generic
over the container trait and then work for any kind of input sent to
it. However, sources sending a single event will require the pipeline to
create an event array, while those sending arrays can avoid that process
of reallocation and moving data.

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Implement `Sink<EventVec> for Pipeline`.
- [ ] Rewrite sink types to accept `EventVec` through wrapper functions.
- [ ] Rewrite task transform type to accept `EventVec` through wrapper functions.
- [ ] Modify `Pipeline` to send `EventVec` to its receiver.
- [ ] Convert sources that can send arrays of events to send `EventVec`.
- [ ] Convert task transforms that can easily process arrays of events to consume `EventContainer`.
- [ ] Convert sinks that can process arrays of events to consume `EventContainer`.

## Future Improvements

It is conceivable that some function transforms could benefit from
processing the array of events internally instead of being implicitly
wrapped by a loop. In such a case, it may be worth adding additional
function transform types that accept either `EventVec` or
`EventContainer` as their inputs instead of a single `Event`. If so,
consideration should be given to replacing the default types with this
and wrapping existing single-event transforms with a loop.

Since producing components will tend to produce events with a similar
structure in each output, a potential optimization to the internal
storage of arrays of events is to have the structure represented once
along with arrays of values. This could be done in two different ways,
both of which have challenges for actually manipulating the data:

1. Replace all leaf values with arrays of scalar values. In this format,
   an event would be composed of many arrays.
2. Flatten out the structure into an array of keys and replace the leaf
   values with an indexed array of arrays.

As referenced at various points above, the core `Event` and `EventVec`
types are enums over log and metric variants. This creates pain for
components that can accept only one or the other, requiring them to
detect the variant at run time. Some thought should go into moving
towards a design that eliminates, or at least reduces, the need for this
wrapper type and instead encodes the capabilities in the type system.
