# Architecture

This document describes the high-level architecture of Vector. It assumes some
familiarity with the user-facing concepts and focuses instead on how they're
wired together internally. The goal is to provide a starting point for
navigating the code and to assist in understanding Vector's behavior and
constraints.

## Overview

From a user's perspective, Vector runs a configuration consisting of a directed,
acyclic graph of sources, transforms, and sinks. In many ways, this logical
representation of the config maps rather directly to the way that it is laid out
and run internally.

![logical](https://user-images.githubusercontent.com/333505/152071180-daa5ea73-d588-4471-b34f-79ffb5e6c295.png)

A reasonably accurate mental model of running the above configuration is that
Vector spins up each component as a Tokio task and wires them together with
channels. Below, we'll go through each type of component in more detail,
discussing how it is translated into a running task and connected to the rest of
the topology.

## Component construction

After parsing and validating a user's configuration, we are left with a `Config`
struct containing (among other things) the collections of `SourceConfig`s,
`TransformConfig`s, and `SinkConfig`s corresponding to each of the configured
components. Each of those traits has their own `build` method that constructs
the component. This building occurs largely in the `src/topology/builder.rs`
file, along with some setup for the initial wiring into the topology.

### Sources

When a source config is built, the result is mainly two tasks: the "server" task
of the source itself, and a "pump" task that forwards its output on to the rest
of the system.

Construction begins by setting up a `SourceSender` for the configured source,
which handles the ability to send events to each of the outputs defined by the
`SourceConfig::outputs` method. The outputs are built into the sender in such
a way that attempting to send to an unknown output will result in a panic.

Along with each of these outputs that are added to the sender, a corresponding
`Fanout` instance is created which will live in a pump task and handling
"fanning out" events to every downstream component that lists this source as an
input. Each of these individual pump tasks (one per output) consists purely of
forwarding events into the `Fanout`. The final pump task (one per source) simply
spawns each of the output-specific pump tasks and drives them to completion.

Finally, the server task itself is built. `SourceConfig::build` takes
a `SourceContext` as an argument, one field of which is the `SourceSender` built
above. The result of the build function is wrapped in some simple shutdown
handling before being inserted into the topology.

![image](https://user-images.githubusercontent.com/333505/156249003-ba6e31de-d296-42da-a9b6-2451e607df80.png)

### Transforms

After sources, transforms are the next to be built. They're a bit simpler than
sources and mostly jump straight into `TransformConfig::build`. They can define
`outputs` that are translated into `Fanout` instances in much the same way as
sources, and also have an option to `enable_concurrency`. How that works exactly
depends on the type of transform.

#### Synchronous

The simplest type of transform is a `Function` transform. They run synchronously
on a single event and write to a single, unnamed output. One step above function
transforms is the newer `Synchronous` transform. These are inclusive of function
transforms, with the additional ability of being able to write to multiple
outputs if desired. From the perspective of the topology, both of these are run
in exactly the same way.

In the simplest case, where the `enable_concurrency` flag is not enabled (this
is not configuration, but a static attribute of the of transform), the resulting
transform task is built to pull a chunk of events from its input channel,
process those events via the `transform` method into a `TransformOutputsBuf`
(essentially a container of `Vec<Event>` for each of the transform's defined
outputs). Once the whole chunk of events has been processed into outputs, those
outputs are drained into the respective `Fanout` instances.

If the `enable_concurrency` flag is enabled, the process is slightly more
complicated. For each chunk of input events, instead of being processed inline,
a new task is spawned that does the work of processing the events into outputs.
Since there is some overhead to spawning tasks, Vector attempts to pull larger
chunks of events from the input for transforms running in this mode. The main
transform tasks then tracks the completion of those work tasks in the same order
that they were spawned (ensuring that we don't reorder the resulting outputs).
When a task completes, the main task receives the resulting
`TransformOutputsBuf` and takes care of draining it into the respective fanouts.
To ensure that we don't buffer an infinite amount of events within those work
tasks, the main task limits the maximum number that can be in flight
simultaneously. Spawning new tasks allows Tokio's work-stealing scheduler to
step in and spread the CPU work across multiple threads when there is a need and
available capacity to do so.

![image](https://user-images.githubusercontent.com/333505/156249361-9a91f61a-445a-403c-92eb-609f2249b3a9.png)

#### Task

A task-style transform differs from the synchronous variants above in that it
has the ability to do arbitrary, asynchronous stream-based transformations of
its input. This includes things like emitting outputs after some timeout,
independent of incoming events. From the topology's perspective, they're quite
simple because they define most of their structure internally and are applied by
basically just passing the input channel into the `transform` method.

To build the full task, the transform itself is built, some common filtering and
telemetry are added by wrapping the input stream, and then the input stream is
passed to the `transform` method. This results in an output stream, which is
then forwarded to the transform's `Fanout` instance (task transforms do not
support multiple outputs).

![image](https://user-images.githubusercontent.com/333505/156249430-5f82a1e0-8caa-49fe-88b8-290b6ed06ad7.png)

### Sinks

Sinks have two components that make building them somewhat more complex than
either sources or transforms: healthchecks and buffers.

Healthchecks are one-off tasks that run at startup with the goal of discovering
any issues that may prevent the sink from running properly (e.g. permissions
or connectivity issues) and notifying the user in a nice way. They can be
enabled or disabled both individually and at a global level, and the user can
choose whether a failing healthcheck should prevent Vector from starting.

Buffers are a configurable mechanism for dealing with backpressure. By default,
like the rest of Vector, sinks will buffer some small-ish number of events in
memory before propagating backpressure upstream. Buffer configuration allows
individual sinks to change that behavior, choosing between memory and disk for
where to store the buffered events, setting a maximum size, and deciding what
should happen when the buffer is full (backpressure or load shedding).

Disk buffers in particular add some complexity in topology construction due to
the fact that they're persistent across both config reloads and process
restarts. They are built normally with their corresponding sink most of the
time, but they are also stashed to the side for the case when topology
construction fails after a buffer has been built. This allows a subsequent build
(likely of the previous configuration during a rollback) to pull from the
already-built buffer without worrying about the persistence of the contents.

Once the healthcheck and buffer are built, the sink itself is constructed via
`SinkConfig::build`. The surrounding task is defined to first finalize its use
of the buffer (removing it from the in-case-of-error stash), filter and wrap the
input stream with telemetry, and then pass it to `VectorSink::run`.

![image](https://user-images.githubusercontent.com/333505/156249509-fd1b1ae6-7193-4fda-a33e-bbd128d63c87.png)

## Connecting components

Once component construction is complete, we're left with a collection of
yet-to-be-spawned tasks, as well as handles to their inputs and outputs. More
specifically, a component input is the sender side of a channel (or buffer) that
acts as the component's input stream. A component output in this case is
a `fanout::ControlChannel`, via which Vector can send control messages that add
or remove destinations for the component's actual output stream.

Given those definitions, the fundamental process of wiring up a Vector topology
is one of adding the appropriate inputs to the appropriate outputs. As a simple
example, consider the following config:

```toml
[sources.foo]
type = "stdin"

[sinks.bar]
type = "console"
inputs = ["foo"]
encoding.codec = "json"
```

After the component construction phase, we'll be left with the tasks for each
component (which we'll ignore for now) as well as a collection of inputs and
outputs. In this case, each of those collections will hold one single item.
There will be an output corresponding to the source `foo`, consisting of
a control channel connected to the `Fanout` instance within the source's pump
task, and there will be an input corresponding to the sink `bar`, consisting of
the sender side of the sink's input channel/buffer (for our purposes here
they're equivalent).

To wire up this topology, the `connect_diff` function on `RunningTopology` will
see that `bar` specifies `foo` as an input, take a clone of `bar`'s input, and
send that to `foo`'s output control channel. This results in the `Fanout`
associated with the source `foo` adding a new sender to its list of destinations
to write new events to. And voilà, the sink will receive events from the source
on its input stream.

The actual code for this logic (found in `src/topology/running.rs`) is
significantly more complex than what's needed for this very simple example,
mostly because it is oriented around applying modifications to an existing
running topology rather than simply starting a new one up from scratch. While
complex, this allows us to have a single path through which all topology actions
occur. Without it, we'd need all the same complexity to support reloads, but it
would be a secondary path that's less well exercised than the common startup
path. Even so, this is an area where we're always looking for ways that we can
simplify and make things more robust.

## Spawning the topology

Once everything is properly connected, the final step is the spawn the actual
tasks for each component. There is a bit of bookkeeping to build tracing spans
for each, set up error handlers, and track shutdown, but the handles to the
running tasks are stored and Vector is off to the races.
