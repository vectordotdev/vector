# RFC 8216 - 2021-07-19 - Multiple Pipelines

Large Vector users often require complex Vector topologies to facilitate the collection and processing of data from many different upstream sources. This results in large Vector configuration files that are hard to manage across different teams. This makes Vector a bad candidate for large teams that require autonomy to facilitate sane collaboration.

## Context

- [RFC 2064](https://github.com/timberio/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md) Event drive observability.

## Scope

### In scope

- How and where pipelines are defined.
- How Vector reads pipelines and what the limitations of pipelines are.
- How we monitor pipelines components.

### Out of scope

- How pipelines should be synchronized between Vector instances.
- Pre/post-processing of data - The ability to prepare and normalize data.
- Connecting pipelines together - The ability to take input from another pipeline.
- Component reuse - The ability to define boilerplate for reuse across many different pipelines. This will likely align with Datadog’s “pipeline catalogue”.
- Pipeline quotas - The ability to limit how much data a pipeline can send to a sink.
- Access control - The ability to control access of global resources (sources and sinks) within pipelines.

## Pain

- It is not possible to delegate the management of pipelines across a matrix of teams and services.
- It is not possible to observe individual pipelines (paths on the graph).
- It is not possible to restrict what pipelines can do (which data they can access, how much data they can send, etc).
- Managing a large graph is cumbersome and usually results in a very large Vector configuration file that is hard to navigate. Splitting the vector config into multiple files is a creative exercise for the user without any guidance.

## User Experience

- These changes provide a way for Vector users to split their configuration in a way that improves the collaboration between ops and devs.
- This split will be made by allowing the creation of individual pipeline configuration files, intended to align with services and teams, enabling autonomous management.
- The ops will now configure their `sources`, `sinks` and `transforms`, and expose them to be used by the devs.
- The devs will have the ability to consume the provided components without being able to change the common configuration.
- The users will be able to monitor their pipelines through the `internal_metrics` source.

## Motivation

- Helps Vector to grow organically within an organization by allowing teams to adopt Vector at their own pace without heavy administrator involvement.
- Reduces the management overhead of devops/SREs by enabling teams to manage their own pipelines (spread the management load).

## Implementation

### How and where pipelines are stored

To avoid backward incompatibility, pipelines will be loaded from a `pipelines` sub-directory relative to the Vector configuration directory (e.g., `/etc/vector/pipelines`). Therefore, if a user changes the location of the Vector configuration directory they will also change the `pipelines` directory path. They are coupled.

The `pipelines` directory will contain all pipelines represented as individual files. For simplicity, and to ensure users do not overcomplicate pipeline management, sub-directories/nesting are not allowed. This is inspired by Terraform's single-level directory nesting, which has been a net positive for simple management of large Terraform projects.

Each pipeline file is a processing subset of a larger Vector configuration file. Therefore, it follows the same syntax as Vector's configuration (`toml`, `yaml`, and `json`).

### What is a pipeline

- A pipeline is a collection of transforms that only have access to the component declared in the root configuration and their own pipeline's transforms.
- A pipeline has an `id` being the name of the file without the extension. The pipeline `load-balancer.yml` will have `load-balancer` as its `id`.
- Pipelines have access to any components from the root configuration. If the transform `foo` is defined in `/etc/vector/bar.toml`, it will be accessible by the pipeline `/etc/vector/pipelines/pipeline.toml`.
- If no pipeline is defined, Vector behaves as if the feature didn't exist. This way, a configuration from a version without the `pipeline` feature will keep working.
- If a pipeline file is left empty, Vector behaves as if it doesn't exist.

If any of the following constraints are not valid, Vector will error on boot. If this occurs during a reload, an error will be triggered and handled in the same fashion as other reload errors.

- There cannot be several pipelines with the same id (for example `load-balancer.yml` and `load-balancer.json`).
- The pipeline's configuration files should only contain transforms.
- A pipeline's transform cannot have the same name as any component from the root configuration.
- A pipeline cannot use another pipeline's component.

### Representation

As mentioned in the previous section, a pipeline is _just_ a set of transforms.

To be able to forward the events going through the pipeline to a `sink`, we need add a new concept of `route`.

`route` components are used solely to build the topology and represent an interface between the topology and the external sinks.

A pipeline will have the following internal representation before building the topology.

```rust
struct Route {
  inputs: Vec<String>,
  outputs: Vec<String>,
}
struct PipelineConfigBuilder {
  id: String,
  transforms: Map<String, TransformOuter>,
  routes: Map<String, Route>,
}
```

Which will create the following configuration file.

```toml
# /etc/vector/pipelines/pipeline.toml
[transforms.foo]
type = "remap"
inputs = ["from-root"]
# ...

[transforms.bar]
type = "remap"
inputs = ["foo"]
# ...

[routes.hot-storage]
inputs = ["foo"]
outputs = ["dc1", "dc2"]

[routes.cold-storage]
inputs = ["bar"]
outputs = ["dc-us", "dc-eu"]
```

The `Route` structure is made to forward the events from inside the pipeline to an external component.

### From configuration to topology

If we look deeper at the configuration building process, the configuration compiler will require the pipelines to build the [configuration](https://github.com/timberio/vector/blob/v0.15.0/src/config/builder.rs#L71).

To do so, we'll need to implement a `PipelineConfigBuilder` from the previous section. We'll then update [the `compile` function](https://github.com/timberio/vector/blob/v0.15.0/src/config/compiler.rs#L4) to build a `Config` containing the required pipeline components. The compiler will load the pipeline's transforms and add the route `outputs` to the corresponding `sinks`.

The components coming from the pipeline will be cloned inside the final `Config`, in the `transforms` `IndexMap` and the `Routes` from the pipeline components will be added to the referring components input field.

For example, the following configuration and pipeline, and its **equivalent** once built.

```toml
# /etc/vector/vector.toml
[sources.in]
# ...

[sinks.out]
# ...

# /etc/vector/pipelines/foo.toml
[transforms.bar]
inputs = ["in"]
# ...

[routes.baz]
inputs = ["bar"]
outputs = ["out"]

# equivalent once compiled
[sources.in]
# ...

[transforms.foo#baz]
inputs = ["in"]

[sinks.out]
inputs = ["foo#baz"]
# ...
```

In order to avoid internal conflicts with the pipeline components `id`s, the components `id`s will be internally prefixed with the pipeline `id` (`pipeline_id#component_id`) but the user can still reference the components with their `id` inside the pipeline configuration.
That way, if a transform `foo` is defined in the pipeline `bar` and in the pipeline `baz`, they will not conflict.

### Observing pipelines

Users should be able to observe and monitor individual pipelines.
This means relevant metrics coming from the `internal_metrics` source must contain a `pipeline_id` tag referring to the pipeline's `id`.
This approach would extend the [RFC 2064](https://github.com/timberio/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md#collecting-uniform-context-data) by _just_ adding `pipeline_id` to the context.

In Vector, once [the topology is built from the configuration](https://github.com/timberio/vector/blob/v0.15.0/src/topology/builder.rs#L106), every component is encapsulated in a `Task`, that intercept any incoming event and does why it has to do. This task also keeps track of its internal metrics and finally emits those `internal_metrics` events.

To add the pipeline information to the task, we need to add a new optional parameter to the [`Task::new`](https://github.com/timberio/vector/blob/v0.15.0/src/topology/task.rs#L29) method.

```rust
pub struct Task {
    #[pin]
    inner: BoxFuture<'static, Result<TaskOutput, ()>>,
    name: String,
    typetag: String,
    pipeline: Option<String>,
}
impl Task {
    pub fn new<S1, S2, Fut>(name: S1, typetag: S2, pipeline: Option<String>, inner: Fut) -> Self
    where
        S1: Into<String>,
        S2: Into<String>,
        Fut: Future<Output = Result<TaskOutput, ()>> + Send + 'static,
    {
        Self {
            inner: inner.boxed(),
            name: name.into(),
            typetag: typetag.into(),
            pipeline,
        }
    }
}
````

That way, when vector [spawns a new transform task](https://github.com/timberio/vector/blob/v0.15.0/src/topology/mod.rs#L574), it will be able to add the optional pipeline information to the span.

```rust
let span = error_span!(
    "transform",
    component_kind = "transform",
    component_name = %task.name(),
    component_type = %task.typetag(),
    pipeline_id = %task.pipeline_id(),
);
```

Doing so, each time the task will emit an internal event, it will be populated by the optional `pipeline_id`.

## Rationale

- Why is this change worth it?

This split improves the readability of the configuration files and allows the users to collaborate, which makes using Vector more user-friendly.

- What is the impact of not doing this?

This would force the users to keep having complex configuration files and/or to duplicate components configuration between their configuration files.

- How does this position us for success in the future?

With this representation, we'll be able add access control by, for example, declaring the pipelines inside the configuration files to limit the reachable components. We would also be able to specify a quota for each pipeline.

## Prior Art

_TODO_

## Drawbacks

- Why should we not do this?
- What kind of ongoing burden does this place on the team?

## Alternatives

- Do nothing: we can already use several configuration files, people could split their existing configuration.

This would imply some duplication if a transform is used in multiple configuration files.
Anybody could add a sink or source even if they don't have permission.

- Do nothing: write a tool that would write a big configuration file where each pipeline would start with a dummy filter that we could monitor in the `internal_metrics`.



- Evolve vector to use a tag/filter model like our competitors, have a 'pipeline' be a 'tag'.

Not able to add internal metrics to specific transforms and monitor them.
Doesn't block to create other sources/sinks.

- Run a single vector per-'pipeline' and support metric tagging to distinguish at the telemetry level.

Adds a lot of complexity and would add some constraints regarding resources that can only be used once.
Doesn't block to create other sources/sinks.

## Outstanding Questions


## Plan Of Attack

- [ ] Create the Pipeline structure and parse a pipeline's configuration file
- [ ] Update compiler to add pipelines as a parameter
- [ ] Update topology with pipeline's components
- [ ] Update the context for taking pipeline information
