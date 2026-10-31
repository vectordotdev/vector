# RFC 12217 - 2022-08-25 - Handling Discarded Events

We want to give Vector users the assurance that events are never lost unexpectedly. There are
currently many circumstances in which events are simply discarded without recourse for users of
Vector to recover those events. This RFC presents a framework for handling those discarded events in
a uniform fashion in all components.

## Context

- [Support dead letter queue sources](https://github.com/vectordotdev/vector/issues/1772)
- [Vector sources acknowledge discarded events](https://github.com/vectordotdev/vector/issues/12217)
- [Reroute discarded events from throttle transform](https://github.com/vectordotdev/vector/issues/13549)
- Sink Error Handling RFC (internal document)

## Cross cutting concerns

- The proposed changes will impact emission and receipt of end-to-end acknowledgements.
- The proposed configuration options should fit in the configuration schema, but may have
  implications for its interpretation, particularly around the topology connections.
- Handling discarded events in any way will change the interpretation of several exiting internal
  metrics.

## Scope

### In scope

- Handling of discarded events in sources, transforms, and sinks.

### Out of scope

- Handling of discarded data in sources before it is translated into events.
- Handling of discarded sink request data after it is translated from events.

## Pain

Vector has a number of ways in which components may drop events due to errors that are outside of
the operator's control:

- Transform processing
- Sink encoding failure
- Sink partitioning failure
- Failure to write to disk buffer

This leads to the potential for lost events and even silently dropped events. Users want to be able
to capture discarded events at various points in the topology in order to forward them to fail-safe
destinations for diagnosis.

## Proposal

There are three parts to the proposed solution:

1. Add a new standardized output to all components that may discard events after errors.
2. Update configuration validation to require that all outputs are handled.
3. Add a new configuration for discarding or rejecting outputs.

### User Experience

#### Add Discarded Event Output

We will introduce a new output to all components that would otherwise discard events, named
`errors`. Note that some components already have such an named output. This proposal standardizes
that output naming and provides additional support for handling it.

#### Validate Handling All Outputs

Additionally, we will enhance the configuration validation to determine if all outputs are
handled. This will provide notification of the new error output and ensure users are aware of the
new feature.

In order to avoid breaking existing configurations, this validation will initially produce a
deprecation warning, notifying the user that an output is present but unhandled. In a later version,
this deprecation will be turned into a hard error, preventing the acceptance of a configuration with
unhandled outputs.

We will also add a new command-line option and environment variable to opt into the stricter
validation for users that want the additional assurance this provides.

#### Simplify Discarded Output Handling

This enforced handling will create considerable complication for most users, as well as the
potential of increased overhead and reduced performance if extra components have to be configured
just to handle the outputs that now need to be connected somewhere. To reduce this overhead, we will
add a new optional configuration setting to indicate the internal disposition of each output:
`outputs.NAME.disposition`. This will have two non-default values, `"drop"` to mark all events going
to that output as delivered and then drop them, and `"reject"` to mark them as having failed.  If
this setting is not present and a component has an unhandled output, a default behavior of `"drop"`
will be assumed, which matches the current behavior. Once validation of output handling is enforced
as above, this default will be removed.

So, to configure the new error output as discarding events with an error result, users would add the
following:

```toml
[transforms.example]
type = "remap"
…
outputs.errors.disposition = "reject"
```

### Implementation

#### Configuration

The new `output.*.disposition` configuration mapping will be added to components as needed to
support configuring outputs. In order for the validation to access it, a new method will be added to
all component configuration traits:

```rust
trait SourceConfig: NamedComponent + Debug + Send + Sync {
    /// Gets the list of outputs exposed by this source (existing).
    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<config::Output>;

    /// The actual user configuration of outputs.
    /// Will be `None` if no such configuration is present on this component.
    fn outputs_config(&self) -> Option<&OutputsConfig>;
}

trait TransformConfig: DescribeOutputs + NamedComponent + Debug + Send + Sync {
    /// Gets the list of outputs exposed by this transform (existing).
    fn outputs(&self, merged_definition: &schema::Definition) -> Vec<config::Output>;

    /// The actual user configuration of outputs.
    /// Will be `None` if no such configuration is present on this component.
    fn outputs_config(&self) -> Option<&OutputsConfig>;
}

trait SinkConfig: DescribeOutputs + NamedComponent + Debug + Send + Sync {
    /// Gets the list of outputs exposed by this sink.
    fn outputs(&self) -> Vec<config::Output>;

    /// The actual user configuration of outputs.
    /// Will be `None` if no such configuration is present on this component.
    fn outputs_config(&self) -> Option<&OutputsConfig>;
}

/// Configuration for a set of named outputs.
struct OutputsConfig(HashMap<String, OutputConfig>);

struct OutputConfig {
    #[serde(default)]
    disposition: OutputDisposition,
}

enum OutputDisposition {
    /// Default behavior, send the event on to further components that name this as an input.
    Send,
    /// Discard the event and mark it is as delivered.
    Drop,
    /// Discard the event and mark it as failed.
    Reject,
}
```

#### Sources

Sources already have facilities to handle a map of named outputs in `SourceSender`. The standard
builder will be extended to add a new standard output to accept discarded events. The `Inner` will be
rewritten to handle either discarding events or passing them on to the sender, and the send
functions will be extended to take the appropriate action based on this disposition.

```rust
struct Inner {
    disposition: InnerDisposition,
    output: String,
    lag_time: Option<Histogram>,
}

enum InnerDisposition {
    Drop,
    Reject,
    Send(LimitedSender<EventArray>),
}
```

#### Transforms

##### "Function" Transforms

Function transforms are the simplest type, taking an event as input and outputting into a single
`OutputBuffer`. This is a strict subset of the synchronous transform type, and adding the necessary
output buffer type would make this type effectively equivalent to synchronous transforms. As such,
all function transforms that may discard events will be rewritten to the synchronous transform type.

##### "Synchronous" Transforms

Synchronous transforms already have support for named outputs. These will be configured with a new
named `errors` output. The output buffer `TransformOutputsBuf` will be extended with additional
convenience methods to making discarding events more straightforward.

```rust
struct TransformOutputsBuf {
    pub fn push_error(&mut self, event: Event);
}
```

##### "Task" Transforms

Each of the current task transforms will be refactored into either a simpler synchronous transform, which
will be handled as above, or into a more specialized `RuntimeTransform` (`aggregate` and `lua` version 2). The
runtime transform type runs inside of a task which, in turn, takes a transform output buffer as an
input parameter. That framework will be rewritten to accept named outputs, with the associated
convenience methods for handling errors. Further, since it is the only instance of a task
transform, that framework will replace the task transform, allowing us to build the outputs in the
topology builder.

- `aggregate` => runtime (flushes on timer tick)
- `aws_ec2_metadata` => synchronous
- `dedupe` => synchronous
- `lua` version 1 => synchronous
- `lua` version 2 => runtime
- `reduce` => runtime (flushes on timer tick)
- `tag_cardinality_limit` => synchronous
- `throttle` => synchronous (uses timers, but only to update state)

##### Output Buffer

The output from transforms is fed into an `OutputBuffer` that is currently a newtype wrapper for a
simple vector of event arrays (ie ```struct OutputBuffer(Vec<EventArray>);```). The additional
output dispositions will be added as enum states:

```rust
pub enum OutputBuffer {
    Drop,                   // Mark events sent to this output as delivered and drop them.
    Reject,                 // Mark events sent to this output as failed and drop them.
    Store(Vec<EventArray>), // Forward the events to the next component, as before.
}
```

#### Sinks

Sinks do not have "outputs" as such. In order to add a error output, we will need to wrap the
incoming events with an optional stream adapter. It will be built and added during topology
building, depending on the presence of an output handler for `errors`. This handler will hold on to
a copy of the events and forward them on to the error output when needed:

1. Clone the input event array.
1. Apply a new finalizer to the cloned events.
1. Send the events to the sink.
1. Wait on the reception of the signal from the finalizer.
1. If the events were delivered, mark the original events (with their original finalizers) as
   delivered. Otherwise, send the events to the next output.

#### Validation

The configuration validation has a routine for checking the outputs of sources and transforms. This
will be enhanced to scan the topology for connectedness. This can be accomplished by setting up a
map of component outputs, marking each where they were seen (in either/or inputs or outputs), and
then scanning that table for entries that are seen as an output but not as an input.

## Rationale

The unified configuration presents a single visible interface to users, much like we do for
end-to-end acknowledgements, despite very different implementations under the hood. This allows for
less cognitive overhead involved in trying to figure out how to configure a component to redirect
discarded events.

The sink wrapper has the major advantage of requiring no changes to existing sinks to produce the
desired effect. Changing each sink to account for events as they flow through, track completion, and
forward to another output on failure would be a major undertaking. Since finalization already tracks
delivery status, we can instead reuse that handling to track this in a different place.

## Drawbacks

While the above proposal avoids rewriting sinks, it does require an audit and/or rewrite on all
sources and transforms. This will be costly to complete. Further, new sources and transforms will
need to abide by the same rules of operation, making them trickier to write except by copying
existing code.

Ideally, we should have some kind of compile-time enforcement on sources and transforms that would
prevent them from dropping events except through the appropriate interfaces. However, preventing the
invocation of `Drop` is only possible with some crude hacks, and can't be scoped to just one part of
the code while avoiding others.

## Alternatives

### Output Disposition

Instead of the output disposition configuration mechanism described above, we could add a simple
shorthand that is specific to the new error output, such as `on_errors: "drop"`. However, there
are components that already have multiple outputs, or at least the option of having multiple
outputs. Once we enforce that all outputs are handled, these will also become an issue for some
users, which recommends the need for a more generic solution.

### Unhandled Output Configuration

Simplifying the configuration of previously unhandled outputs presents a bit of a Catch-22. As it
stands now, if we just add additional outputs and require all outputs to be handled, we would
require users to explicitly route the unhandled outputs to a new `blackhole` sink or equivalent. So,
we want to add a shorthand to avoid the extra configuration that would require, and potentially the
extra running component internally.

The form of that shorthand presents a bit of a conundrum, though.  The configuration for consuming
outputs happens in the `inputs` section of downstream components. If we add the discard
configuration on the outputting components, that could create a configuration conflict of naming a
blackhole output as an input.  ie

```toml
[sources.sample1]
…
outputs.errors = "discard"

[sinks.sample2]
inputs = ["sample1.errors"]
```

If, on the other hand, we add the discard configuration outside of the outputting component, that
isn't much of a simplification over just creating the blackhole component. ie

```toml
[sources.sample1]
…

[blackhole_outputs]
inputs = ["sample1.errors"]
```

The proposed form was chosen as it allowed for the maximum flexability for configuring the discarded
outputs.

### Function Transforms

The "function" transform type could also be modified to take a new output type containing both the
output buffer and error buffer. This leaves the existing transform framework setup in place, but
increase the amount of work required to complete this project.

### Task Transform

We could potentially use the same kind of stream wrapper as for sinks in the task transform, which
receives and emits a stream of events. However, we would need to also catch the events on the other
side and reattach the original finalizers and drop the copied events, making this process much more
complex than for sinks, where the events don't have another output.

### Sink Wrapper

Instead of a wrapper running in front of sinks, we could pass an output buffer into sinks, much like
transforms, and simply modify them to write failed events to that output. This would have the
benefit of higher performance, as we could likely rework most existing sinks to avoid needing the
clone and would not need the extra finalizer and task to handle forwarding the events. On the other
hand, this performance loss appears to be relatively minor, likely in the low single digit percent
range. This work would also need to done to each sink in turn which, combined with the potential for
substantial modifications to each sink to save copies of the events, would add up to a large
effort. This path could also increase the amount of ongoing maintenance required due to the
increased complexity of the code base.

We could potentially also integrate more closely with the buffer system that already sits in front
of sinks. Disk buffers already have a reasonable amount of overlap with what we’d like to do, since
they store copies of events until they have been positively acknowledged. The downside is that
memory buffers do not work this way and it would be a significant amount of work to make them work
this way. It’s also not clear how much of a performance hit this would involve, and changing how
memory buffers work would invoke that performance penalty between nearly every single component in
every Vector configuration. It’s still possible that some level of conditional integration with
buffers would be a good approach, but that path towards that is less clear than solving the problem
in isolation.

## Outstanding Questions

- Do we want to automate the process of rewriting configurations that have missing output handlers,
  or is it adequate to just have a configuration validator?

- What should be done with the existing discarded event metrics? Should they always be emitted, or
  only when the output isn't consumed by another component? Do we need another disposition marker to
  indicate discards are not to be counted as errors?

- Does the list of outputs exposed by the sink need the schema definition passed in like transforms
  do?

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Add new output configuration settings.
- [ ] Add output disposition and new error output to `SourceSender`.
- [ ] Update sources (`kubernetes_logs`) to discard events through the output buffers.
- [ ] Update transform `OutputBuffer` to add non-send dispositions.
- [ ] Add error output to function transform.
- [ ] Add error output to sync transform.
- [ ] Convert function transforms to synchronous transforms.
- [ ] Update sync transforms to discard events through the output buffers.
- [ ] Make runtime transforms a primitive transform type.
- [ ] Rewrite task transforms in terms of other transform types.
- [ ] Drop task transform type.
- [ ] Verify configured output connectedness.
- [ ] Eliminate `Dropped` event status.

## Future Improvements

We could potentially create a custom lint using [the dylint
framework](https://www.trailofbits.com/post/write-rust-lints-without-forking-clippy) to prevent code
that drops events in sources and transforms (and indirectly elsewhere) from compiling.

We may want to consider lifing the option for turning unhandled output enforcement into a hard error
into a more generic switch to turn all deprecations into errors. This is beyond the scope of this
proposal, as the mechanism for doing so will vary greatly across the different locations where
deprecations are present.
