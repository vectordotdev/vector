# RFC #2625 - Architecture Rework

## Motivation

With the potential for nearly unlimited scope creep, it's important to lay out
specific goals for this RFC and tie them directly to actions.

### Core goals

The following goals are the fundamental motivations for taking on this project.
Any proposed course of actions should be sure to address them all thoroughly.

#### Performance via concurrency

First and foremost, the changes resulting from this RFC should enable
significant performance improvements by unlocking better concurrency. Many
configurations currently bottleneck on a single transform due to the way we give
each component a single task.

We also should make sure that our solution here scales down as well as it scales
up. Vector should strive to have as small a footprint as possible when it is not
under high load and use resources only as needed. As much as possible, we do not
want to require users to configure things like thread pool sizes based on their
expected load.

#### Modular topology building

Topology building and reloading is some of the most complex logic in the Vector
codebase. In order to support the additional complexity of concurrency, we need
to make a concerted effort to simplify and modularize as much as possible.
Adding concurrency cannot come at the cost of maintainability.

We have also seen multiple bugs reported around edge cases of topology
reloading. This is a good time to reconsider our strategy for that functionality
and decide if we could simplify while maintaining the most important user-facing
benefits.

#### Improved quality and consistency

In the early days of Vector's development, we deliberately resisted introducing
much overarching structure. With our limited knowledge of what Vector would
become, we wanted to avoid introducing anything that would turn out to be [the
wrong
abstraction](https://www.sandimetz.com/blog/2016/1/20/the-wrong-abstraction).

At the current stage of the project, however, we have more than enough
information to make informed decisions and the lack of structure is proving to
be burden both for maintenance and onboarding new contributors. It's time to
take what we've learned and put into place concrete patterns that make our code
more consistent, less duplicative, and more strict in enforcing correct
behavior.

### Stretch goals

In addition to the goals above, there are a few enhancements we should keep in
mind while doing this design work. We don't necessarily seek to accomplish them
as part of the same work, but we should help to lay the groundwork to make them
possible and ensure we don't paint ourselves into a corner where they become
overly difficult to accomplish later.

#### Better branching

We currently have a swimlanes transform that allows users to encode logical
branching. This is a very valuable feature but had to be shoehorned into our
existing topology model. Implementation-wise, it's not actually a transform at
all, but a macro that expands to multiple transforms. While this is clever and
useful, it is far from optimal in terms of complexity, performance, etc. With
our work on the topology system, we should think about ways to support this more
directly in the future.

#### End-to-end acknowledgement

For certain sources like Kafka, Vector's usefulness would be dramatically
improved with the ability to precisely acknowledge messages once they have been
fully processed. This would provide far more meaningful guarantees against data
loss than we are able to provide today.

We currently provide a way for sinks to propagate acknowledgements back to their
disk buffer, but a real solution to this problem would need to span the entire
pipeline. This also opens up questions about acknowledging messages that fan out
to multiple sinks, etc.

#### Multi-tenancy and hosted environments

Vector was designed to be run continuously on a single host in a single
organization's infrastructure. While this will like always be our primary use
case, a few others have come up that would be beneficial to enable.

One is multi-tenancy. What would it look like for a Vector process to support
not just one topology, but many, across many organizations? This would likely
push topologies to be more data-like and less physical collections of running
tasks. Credential management would likely be a challenge.

A closely related concern is the runtime behavior in a hosted HTTP-based
environment. Instead of immediately returning and continuously processing
events, it would make sense for incoming batches to be fully processed and sent
upstream before responding with 200 OK.

## Proposal

In short, we should address these issues by decoupling our user-facing model of
sources, transforms, and sinks from our internal runtime task layout. While
there is still plenty we can do to improve straight-line performance, the most
dramatic gains will come from organizing work more intelligently.

As an analogy, consider a system like PostgreSQL. The user-facing model of
relatively simple operators (e.g. `SELECT`, `WHERE`,  `JOIN`) is transformed
into a much more specific, optimized query plan. Predicates from a `WHERE`
clause are pushed down, the theoretical `JOIN` becomes a concrete hash join,
merge join, or nested loop, etc, etc. Vector will not need the same level of
sophistication as PostgreSQL's query planner and optimizer, but the idea of
processing user input into an intelligent execution plan is the same.

Concretely, this will affect both the internal design of our components as well
as the way that we combine them into topologies.

### Component Design

Our current component designs are extremely bare-bones and reflect the way that
we directly turn them into physical tasks. To be able to plan more
intelligently, we'll need a larger, more precise vocabulary.

#### Sources

Of all the component types in Vector, sources are the least effected by this
proposal. They are currently modeled as essentially a bare `Task` that is given
an output channel. This reflects the wide variety of source implementations,
from HTTP servers to file tailing to stdin.

The only thing we seek to change with this proposal is the implementation of the
output channel. Instead of a literal concrete channel sender, sources will
receive an implementation of a new `Push` trait (similar to the one in the
[`timely_communication`
crate](https://docs.rs/timely_communication/0.11.1/timely_communication/trait.Push.html).
The API will be roughly the same as before, but give us more flexibility in
implementation behind the scenes.

#### Transforms

Transforms are currently represented by a single trait with three methods
(`transform`, `transform_into`, and `transform_stream`). These methods roughly
represent different levels of capabilities and a given implementation should
really only use one of them.

With this change, we will break up the singular `Transform` trait into a number
of different traits that directly represent these various capabilities (as well
as some new ones). Initially, the biggest differentiators will be:

1. Persistent state / ordering
2. Ability to produce events independently of incoming events

The first is currently implicit and we assume all transforms could be accumulate
state and require strict ordering for correct execution. Since we currently use
literal serial execution, this is supported by default.

The second was the original impetus for `transform_stream`. This determines
whether a transform can be a simple function like `map` or `filter`, or if it
needs to be its own task that can be scheduled independently. Currently all
transforms are run as a task either way, and this is just  a matter of
implementation convenience.

Our model of transforms will adapt into something like the following pseudocode:

```rust
trait TransformConfig {
    async fn build(&self) -> Transform
}

enum Transform {
    Function(FunctionTransform),
    Task(TaskTransform),
}

trait FunctionTransform {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event);
}

trait TaskTransform {
    fn transform(self: Box<Self>, stream: Stream<Event>) -> Stream<Event>;
}
```

Instead of `build` returning a single opaque transform that always gets run as
its own task, we have the ability to differentiate between different types of
transforms. This gives us the option to treat them differently during topology
construction.

#### Sinks

Sinks are where we've had the most luck introducing reusable abstractions so
far. They also don't currently appear to be much of a performance bottleneck, so
the concerns here are somewhat independent of (or at least not as urgent as)
those above.

The most obvious problem with our current sinks is that we directly reuse the
`Sink` trait from the `0.1` version of the `futures` crate. This will obviously
need to change before we can complete our transition to the `0.3` version of
`futures`.

One option would be to simply transition to the `0.3` version of the trait, but
that is undesirable for a number of reasons. First, the new version is
significantly more complex. Second, there is a general sentiment in the
community that the trait is not worth the complexity and is unlikely to be
stabilized in the way that others have.

Instead, we should migrate towards our own type that both insulates us from this
ecosystem uncertainty and more closely aligns with our specific needs.

```rust
trait SinkConfig {
    async fn build(&self) -> Result<Sink>;
}

enum Sink {
    Streaming(StreamingSink),
    Service(ServiceSink),
    OldAndBad(futures01::Sink),
}

impl Sink {
    async fn run(self, input: Stream<Event>) -> Result<()> {
        ...
    }
}
```

Again, this is just pseudocode, but it illustrates a couple of important points:

1. There are a number of types of sinks that can differ in implementation details
2. We provide a simple, unified, stream-based interface to drive all of them

The most important benefit here is that we don't need to rewrite everything at
once to move to a new style. You can simply wrap `Sink::OldAndBad` around
existing implementation and upgrade them later one at a time.

Another important point is that we've decoupled the topology-level API from the
implementation-level API. This means we can start with something simple like the
`run` method above, and later move towards something more sophisticated without
necessarily touching every individual component.

Once this seam is established, the next most important abstraction is
`ServiceSink`. We have something similar right now in `BatchSink`, but we've
begun to see limitations of that design. A full proposal is outside of the scope
of this RFC, but future work should explore the relationship between batching,
encoding, and request building. Their separation in the current design appears
to be the cause of some limitations and there's likely an improvement to be made
by evaluating and adjusting to their mutual dependencies.

### Topology design

With the above component changes in place, we'll gain some flexibility in how we
build topologies. The goal will be, where possible, to consolidate stateless
transform-like processing into clonable `Push` implementations that are passed
to sources in place of the current raw channels senders. This will effectively
"inline" this logic into the source task, allowing it to execute with the
natural concurrency of a given source (e.g. per connection).

#### Runtime task layout

For example, consider a Vector configuration with a TCP syslog source, grok
parser, rename fields transform, and Cloudwatch logs sink. With our existing
design, each of those components exists as a single independent task, connected
together with channels. While simple, this leads to the whole pipeline being
limited by the single-threaded throughput of the slowest component.

In the new design, we take advantage of the fact that all of the transform work
for this config is perfectly parallelizable. Where the source would normally
clone a channel `Sender` for each incoming connection, it will now clone
a `Push` implementation called `Pipeline` (naming TBD) that does the
transformation work inline before forwarding the results directly to the sink.
This allows our CPU concurrency to scale up naturally with the natural
concurrency of the source.

In an abstract sense, the goal of this change is to transition from a topology
of individual, heterogeneous tasks to one of many consolidated, homogeneous
tasks. Take for example the below topology:

![Example topology](./2020-06-18-2625-architecture-revisit/combined.png)

By shifting the stateless transforms up into the connection handling tasks, we
achieve significantly increased CPU concurrency for that computation. Transforms
that are stateful remain as their own tasks, but tend to come later in most
pipelines.

#### Pipeline compilation

This new approach is not applicable to all types of transforms, so we can't
naively combine all of them into a `Pipeline` and call it a day. A vital part of
this work will be introducing a compilation step in between config parsing and
topology construction. This step will take advantage of the increased visibility
provided by the new component designs to intelligently consolidate relevant
transforms and leave the rest as independent tasks.

By traversing the configured topology graph outwards from each source, we can
identify the largest continuous set of nodes that are eligible to be
consolidated (e.g. are stateless transforms). We can then build those nodes into
a `Pipeline` to be attached to the source and adjust the remaining nodes' inputs
to hook into the output of the pipeline instead of an independent task.

Given that Vector configurations have the potential to be quite complex, this
implementation will not be trivial. Luckily, it is very amenable to starting
simple and incrementally covering more and more cases over time. An initial
implementation of this compilation step could do something as simple as
expanding wildcards in `inputs` arrays or running our existing type checks. This
would allow us to carve out a spot without dealing with the full complexity, and
then slowly expand to optimizing straight-line graphs, then those with branches,
etc, etc.

One particularly interesting piece of complexity is pipelines that fan out. This
would be the first case of a single task having multiple outputs. By ensuring
that our internal model supports this well, we can lay the groundwork for
branching as a user-facing feature.

As we build out this compilation functionality, we should take the opportunity
to keep all the transformations as modular as possible. Similar to phases in
a traditional compiler, we can take the complex user input of our TOML config
and iteratively consolidate and lower it to a simpler representation. Each step
can be independently written and tested, and we can even make certain
optimizations opt-in experiments or configurable.

## Prior art

The idea of running full copies of the processing graph in each worker
thread/task comes from [timely
dataflow](https://www.youtube.com/watch?v=yOnPmVf4YWo), [Kafka
Streams](https://kafka.apache.org/25/documentation/streams/architecture#streams_architecture_threads),
and
[Logstash](https://www.elastic.co/guide/en/logstash/current/execution-model.html)
(to a lesser degree). In each case, the strategy was to increase throughput via
concurrency enabled by partitioning. Homogeneous workers help reduce complexity
of resource allocation, lower communication overhead, etc.

Timely and Kafka Streams are both libraries rather than configurable tools, so
there was less to draw from in terms of dynamic topology building and/or
compilation.
[Logstash](https://github.com/elastic/logstash/blob/78c7204552d893fe176e516cd923bf0cc1f4d052/logstash-core/src/main/java/org/logstash/config/ir/ConfigCompiler.java),
on the other hand, is similarly configurable and has a whole compiler-like
infrastructure with IR, etc, to build what they call "datasets" (like our idea
of pipelines above).

Some other interesting systems that I wasn't able to dig into as deeply are
[Materialize](https://github.com/MaterializeInc/materialize) and
[declarative-dataflow](https://github.com/comnik/declarative-dataflow). Both
build on top of timely dataflow, but do so in a way that computation is defined
at runtime instead of compile-time. There is likely some useful knowledge to be
gained by investigating how they build processing topologies at runtime. On the
other hand, both system focus significantly more on computation than Vector,
which focuses more on data movement.

## Sales Pitch

The changes proposed above put us in a much stronger architectural position
moving forward. The most important differences are focused on increasing the
amount of information available at runtime and allowing us to make more
intelligent decisions. Even if the specific implementation choice of pushing
transform into source tasks does not work out, the same interface changes would
allow alternative options (e.g. a threadpool) with no change to components
themselves.

The concept of pipelines per source task gives us a concurrency model that
scales naturally with load while taking advantage of natural partitioning of the
data. It can be extended later with some user-facing idea of partitioning, but
we can start to reap the benefits of partitioning without putting that burden on
users.

Sources with embedded pipelines will very quickly start to look like a single
topology node with multiple outputs. By modelling this specifically in our
internals, we have a natural place to start experimenting with programmable
control flow. This experience and groundwork will be invaluable if/when we
choose to add it as a user-facing feature.

Formalizing the idea of a staged, compiler-like path from raw config files to
running topologies gives us the opportunity to clean up and better organize
a number of existing features. Environment variable interpolation, multiple
config files, type checking, etc are examples of potential stages that could
be much better implemented in this framework. Other requested features like
snippets, input wildcards, etc would also be much easier to implement with this
in place.

Introducing different types of sinks (e.g. streaming vs request-response) gives
us the opportunity to consolidate shared behavior from sinks of the same type.
Right now, service-based sinks all do their own work to build up middleware
stacks for rate limiting, retries, concurrency, etc. This could be pushed up
into topology building if we have enough information to differentiate these
sinks for those that do not need these wrapping layers. It also opens up the
possibility of using these sinks in alternative runtime setups, like a hosted
HTTP service. Instead of being wrapped in a `Sink` -like structure, services
could be called directly from an HTTP handler.

## Drawbacks

There are a number of downsides to the changes proposed here:

1. We are adding both implementation complexity as well as increasing the size
   of the mental model needed to understand Vector's behavior.

2. It's possible that tying computational concurrency to source task concurrency
   results will work well in some situations and work poorly in others. It adds
   a significant level of variability to something that used to be relatively
   consistent. This applies to both performance as well as memory use.

3. Many of the benefits here will not come automatically. We'll need to make
   sure that we keep the overall design in mind and make individual components
   that work well with it. This applies particularly to sources that don't
   currently run more than one task (e.g. file source, UDP-based sources, etc).

4. It's not entirely clear how runtimes like Lua and WASM should fit into this
   model. Their biggest benefit is the flexibility to do anything, which goes
   against our strategy of limiting transform capability. They also have
   relatively unknown memory requirements, which could make them unsuitable to
   run potentially thousands of copies of in a high-concurrency server scenario.
   The best solution here may be something like the Lua transform's hooks, which
   we can inspect at runtime to determine the required capabilities.

## Rationale and Alternatives

The rationale for these changes is to give us the maximum amount of future
flexibility for the least amount of work. If we can isolate as much code as
possible from the effects of future changes, we should be in a good position to
move forward smoothly.

The biggest alternative to doing this kind of reworking would be to stick with
the existing overall model and try to work things like concurrency into each
individual component. One example proposal along these lines was to have
transforms batch up events and spawn a new task to process them. This would
likely require some degree of similar changes in terms of defining types of
transforms, but would keep topology-level architecture untouched.

Making fewer, less invasive changes would have the strong benefit of being
cheaper, but I worry that the improvements we'd be able to make with such
a limited view (i.e. only within a single component) would keep us rather
limited.

When considering specifically the change to push transforms into source tasks,
there are a number of alternatives for getting that kind of concurrency. While
I believe that design has the most promise, one benefit of this proposal is that
it becomes an implementation detail. The other changes around component and
topology design should give us the freedom to experiment with different ways of
achieving concurrency under the hood.

## Outstanding Questions

1. Where do runtimes fit into the new model of transforms?

2. Should we (and if so, how should we) add concurrency to components like the
   file source and UDP-based socket and syslog?

3. Should we limit the total amount of concurrency somehow? Is that possible
   when it's tied directly to source tasks?

4. How difficult will it be to represent configs with arbitrary branching in
   a single `Pipeline` struct?

5. How exactly should we model sources with pipelines as a component with
   multiple outputs?

6. How can we simplify the model of a running topology such that the code
   managing them becomes simpler? Will consolidation into fewer tasks be enough
   to see benefits?

7. Will adding a compilation step complicate the config reload process? Will it
   be enough to ensure the resulting struct be diff-able in the same way our
   config is currently?

## Plan of Attack

The following can all begin in parallel:

- [ ]  Migrate `build` methods to be async

- [ ]  Introduce minimal `Sink` wrapper enum with single variant for `futures
0.1` sinks

- [ ]  Introduce minimal `Transform` wrapper enum with single `Task` variant

- [ ]  Introduce `Push` trait to be passed to sources, starting with existing
channel `Sender` as only implementation (alternatively, `Pipeline` enum if we
run into generics or object safety issues)

- [ ]  Organize existing ad-hoc config processing stages (env var interpolation,
multiple file support, type checking) into a more formal compiler design ready
to be extended

With those basics in place, we can start to expand each of them:

- [ ]  Add additional `Sink` variants for streaming and services, still mapping
each back to a task

- [ ]  Add additional `Transform` variants for stateless and stateful fns, still
mapping each back to a task

- [ ]  Add new config processing stage to expand wildcards in `inputs`, ensuring
the model supports that kind of mutation without breaking existing stages

From that point on, the work will be split into two separate workstreams:

- [ ]  Adjust relevant components to return a more specific version of their
`Sink` or `Transform` trait (straightforward, very parallelizable)

- [ ]  Move conversion into tasks out of `Sink` and `Transform` and into
a config compiler stage, and evolve to consolidate into pipelines where possible
instead of always transforming to tasks (more in-depth, single threaded dev
work)
