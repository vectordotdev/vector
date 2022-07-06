# RFC 8216 - 2021-07-19 - Multiple Pipelines

Large Vector users often require complex Vector topologies to facilitate the collection and processing of data from many different upstream sources. Currently, this results in large Vector configuration files that are hard to manage, especially across different teams. This RFC lays out the concept of pipelines, a structured way to organize configuration that makes Vector a better candidate for use cases involving widespread collaboration on configuration.

## Context

- [RFC 2064](https://github.com/vectordotdev/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md) Event driven observability.

## Scope

### In scope

- The definition of pipelines and their limitations
- How pipelines fit into Vector's configuration loading and topology
- Expected observability outputs related to pipelines

### Out of scope

- Access control - The ability to control access of global resources (sources and sinks) within pipelines.
- Component reuse - The ability to define boilerplate for reuse across many different pipelines. This will likely align with Datadog’s “pipeline catalogue”.
- Connecting pipelines together - The ability to take input from another pipeline.
- Pipeline quotas - The ability to limit how much data a pipeline can send to a sink.
- Pre/post-processing of data - The ability to prepare and normalize data.
- How pipelines should be synchronized between Vector instances.

## Pain

- Vector does not provide the ability to enforce any kind of organizational structure on configuration files, making large configurations painful.
  - Because of this lack of structure, this is no clear path to achieving delegation and/or isolation of configuration subsections.
- There is no means for grouping individual components together for observability purposes.

## Proposal

### User Experience

This change will introduce the concept of pipelines to users. A pipeline is defined as:

1. A collection of transforms defined together, outside of the top-level configuration file
1. Able to draw input from and send output to components defined in the top-level configuration, but are isolated from other pipelines
1. Having each contained component's internal metrics tagged with the `id` of the pipeline

Pipelines will be loaded from a `pipelines` sub-directory relative to the Vector configuration directory (e.g., `/etc/vector/pipelines`). Therefore, if a user changes the location of the Vector configuration directory they will also change the `pipelines` directory path. They are coupled.

The `pipelines` directory will contain all pipelines represented as individual files. For simplicity, and to ensure users do not overcomplicate pipeline management, sub-directories/nesting are not allowed. This is inspired by Terraform's single-level directory nesting, which has been a net positive for simple management of large Terraform projects.

Each pipeline file is a processing subset of a larger Vector configuration file. Therefore, it follows the same syntax as Vector's configuration (`toml`, `yaml`, and `json`). Each pipeline will have an `id` derived the name of the file without the extension. For example, the pipeline defined in `load-balancer.yml` will have `load-balancer` as its `id`.

Pipelines have access to any components defined in the root configuration directory. For example, if the transform `foo` is defined in `/etc/vector/bar.toml`, it will be accessible by the pipeline `/etc/vector/pipelines/pipeline.toml`, but if a transform `bar` is defined in `/etc/vector/pipelines/another-pipeline.toml`, it will _not_ be accessible by other pipelines.

If no pipeline is defined, Vector behaves as if the feature didn't exist. This way, a configuration from a version without the `pipeline` feature will keep working. If a pipeline file is left empty, Vector behaves as if it doesn't exist.

If any of the following constraints are violated, Vector will error on boot:

- There cannot be several pipelines with the same id (for example `load-balancer.yml` and `load-balancer.json`).
- The pipeline's configuration files should only contain transforms.
- A pipeline's transform cannot have the same name as any component from the root configuration.
- A pipeline cannot use another pipeline's component as input or output.

If the violation occurs during a reload, an error will be triggered and handled in the same fashion as other reload errors.

### Implementation

#### Internal representation

As mentioned in the previous section, a pipeline is _just_ a set of transforms.

To be able to forward the events going through the pipeline to a `sink`, we'll add a new option `outputs` on the pipeline's transforms that will simply specify where the transforms events are redirected to.

`outputs` options are used solely to build the topology and represent an interface between the transform and the external sinks.

A pipeline will have the following internal representation before building the topology.

```rust
struct PipelineTransform {
  inner: TransformOuter,
  outputs: Vec<String>,
}

struct Pipeline {
  id: String,
  transforms: Map<String, PipelineTransform>,
}
```

Which corresponds to the following configuration file:

```toml
# /etc/vector/pipelines/pipeline.toml
[transforms.foo]
type = "remap"
inputs = ["from-root"]
outputs = ["dc1", "dc2"]
# ...

[transforms.bar]
type = "remap"
inputs = ["foo"]
outputs = ["dc-us", "dc-eu"]
# ...
```

The `outputs` option is made to forward the events from inside the pipeline to an external component.

#### From configuration to topology

If we look deeper at the configuration building process, the configuration compiler will require the pipelines to build the [configuration](https://github.com/vectordotdev/vector/blob/v0.15.0/src/config/builder.rs#L71).

To do so, we'll need to implement a `Pipeline` from the previous section. We'll then update [the `compile` function](https://github.com/vectordotdev/vector/blob/v0.15.0/src/config/compiler.rs#L4) to build a `Config` containing the required pipelines components. The compiler will load the pipelines' transforms and add the `outputs` to the corresponding `sinks`.

The components coming from the pipeline will be cloned inside the final `Config`, in the `IndexMap` containing the `transforms` and the `outputs` from the pipeline components will be added to the referring components input field.

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
outputs = ["out"]
# ...

# equivalent once compiled
[sources.in]
# ...

[transforms.foo#baz]
inputs = ["in"]

[sinks.out]
# the # notation is just a representation of the pipeline namespace
inputs = ["foo#baz"]
# ...
```

In order to avoid internal conflicts with the pipeline components `id`s, the components `id`s internal representation will be changed to the following `struct`

```rust
struct ComponentId {
    name: String,
    scope: ComponentScope,
}

enum ComponentScope {
    Global,
    Pipeline(String),
}
```

That way, if a transform `foo` is defined in the pipeline `bar` and in the pipeline `baz`, they will not conflict.

#### Observing pipelines

Users should be able to observe and monitor individual pipelines.
This means relevant metrics coming from the `internal_metrics` source must contain a `pipeline_id` tag referring to the pipeline's `id`.
This approach would extend the [RFC 2064](https://github.com/vectordotdev/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md#collecting-uniform-context-data) by _just_ adding `pipeline_id` to the context.

In Vector, once [the topology is built from the configuration](https://github.com/vectordotdev/vector/blob/v0.15.0/src/topology/builder.rs#L106), every component is encapsulated in a `Task` that intercepts an incoming event and processes it accordingly. This task also keeps track of its internal metrics and finally emits `internal_metrics` events.

To add the pipeline information to the task, we need to change the `name` parameter to `id: ComponentId` in the [`Task::new`](https://github.com/vectordotdev/vector/blob/v0.15.0/src/topology/task.rs#L29) method.

```rust
pub struct Task {
    #[pin]
    inner: BoxFuture<'static, Result<TaskOutput, ()>>,
    id: ComponentId,
    typetag: String,
}

impl Task {
    pub fn new<S, Fut>(id: ComponentId, typetag: S, inner: Fut) -> Self
    where
        S: Into<String>,
        Fut: Future<Output = Result<TaskOutput, ()>> + Send + 'static,
    {
        Self {
            inner: inner.boxed(),
            id,
            typetag: typetag.into(),
        }
    }
}
````

That way, when vector [spawns a new transform task](https://github.com/vectordotdev/vector/blob/v0.15.0/src/topology/mod.rs#L574), it will be able to add the optional pipeline information to the span.

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

Why is this change worth it?

- These changes provide a way for Vector users to split their configuration in a way that improves the collaboration between ops and devs.
- This split will be made by allowing the creation of individual pipeline configuration files, intended to align with services and teams, enabling autonomous management.
- The ops will now configure their `sources`, `sinks` and `transforms`, and expose them to be used by the devs.
- The devs will have the ability to consume the provided components without being able to change the common configuration.
- The users will be able to monitor their pipelines through the `internal_metrics` source.
- Helps Vector to grow organically within an organization by allowing teams to adopt Vector at their own pace without heavy administrator involvement.
- Reduces the management overhead of devops/SREs by enabling teams to manage their own pipelines (spread the management load).

What is the impact of not doing this?

- This would force users to maintain complex configuration files and/or to duplicate component configuration across configuration files.

How does this position us for success in the future?

- With this representation, we'll be able add access control by, for example, declaring the pipelines inside the configuration files to limit the reachable components. We would also be able to specify a quota for each pipeline.

## Drawbacks

- Why should we not do this?
- What kind of ongoing burden does this place on the team?

## Alternatives

- Do nothing: we can already use several configuration files, people could split their existing configuration.

This would imply some duplication if a transform is used in multiple configuration files.
Anybody that has write access to Vector's configuration folder could add a sink or source.
Adding a different folder would allow to separate concerns between a `root` config and a `pipeline`.

- Do nothing: write a tool that would write a big configuration file where each pipeline would start with a dummy filter that we could monitor in the `internal_metrics`.

Writing a different tool would increase the difficulty of using this feature.
Doesn't add access control regarding who can edit the `root` config.

- Evolve vector to use a tag/filter model like our competitors; have a 'pipeline' be a 'tag'.

This doesn't allow to add internal metrics to specific transforms and monitor them, other than by adding a dummy filter that we could monitor.
Doesn't add access control regarding who can edit the `root` config.

- Run a single vector per-'pipeline' and support metric tagging to distinguish at the telemetry level.

Adds a lot of complexity and would add some constraints regarding resources that can only be used once.
Doesn't block to create other sources/sinks.

## Outstanding Questions


## Plan Of Attack

- [ ] Create the Pipeline structure and parse a pipeline's configuration file
- [ ] Update compiler to take the pipelines into consideration during validation
- [ ] Update topology with pipeline's components
- [ ] Update the context for taking pipeline information

## Future Improvements

- Implement a mechanism to avoid Vector to stop when a pipeline is misconfigured. This could be done by just logging the error and ignoring the pipeline.
- Allow to customize the pipelines location.
