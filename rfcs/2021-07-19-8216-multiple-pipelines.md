# RFC 8216 - 2021-07-19 - Multiple Pipelines

Large Vector users often require complex Vector topologies to facilitate the collection and processing of data from many different upstream sources. This results in large Vector configuration files that are hard to manage across different teams. This makes Vector a bad candidate for large teams that require autonomy to facilitate sane collaboration.

## Context

- [RFC 2064](https://github.com/timberio/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md) Event drive observability.

## Scope

### In the scope

- How and where the pipelines are defined
- How vector reads the pipelines and what are the limitations of pipelines
- How do we monitor pipelines components

### Out of scope

- How the pipelines should be synchronized between vector instances
- Pre/post processing of data - The ability to prepare and normalize data.
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

- These changes are providing a way for vector users to split their configuration in a way that improves the collaboration between ops and devs.
- This split will be made by allowing the creation of individual pipeline configuration files, intended to align with services and teams, enabling autonomous management.
- The ops will now configure their `sources` and `sinks`, and expose them to be used by the devs.
- The ops will be able to only given access to some of the common components when configuring the pipeline.
- The devs will have the ability to consume the provided components without being able to change the common configuration.
- The users will be able to monitor their pipelines through the `internal_metrics` source.

## Motivation

- Helps Vector to grow organically within an organization by allowing teams to adopt Vector at their own pace without heavy administrator involvement.
- Reduces the management overhead of devops/SREs by enabling teams to manage their own pipelines (spread the management load).

## Implementation

### How and where the pipelines are stored

To avoid backward incompatibility, pipelines will be loaded from a `pipelines` sub-directory relative to the Vector configuration directory (e.g., `/etc/vector/pipelines`). Therefore, if a user changes the location of the Vector configuration directory they will also change the `pipelines` directory path. They are coupled.
The `pipelines` directory will contain all pipelines represented as individual files. For simplicity, and to ensure users do not over complicate pipeline management, sub-directories/nesting are not allowed. This is inspired by Terraform's single-level directory nesting, which has been a net positive for simple management of large Terraform projects.
Each pipeline file is a processing subset of a larger Vector configuration file. Therefore, it follows the same syntax as Vector's configuration (`toml`, `yaml`, and `json`).

### Configuration

Each time Vector will read its configuration file (on boot and when it live reloads), it will first read the pipeline configuration files and associate an `id` attribute corresponding to the pipeline's filename.

The filename is used to point out the pipeline configuration file in the `pipelines` folder.
In the pipeline's configuration file, the `id` can be set manually.
A `load-balancer.yml` will, by default, have `load-balancer` as `id`.
If several files having the same `id` in that folder (for example `load-balancer.yml` and `load-balancer.json`), vector will error on boot. If this occurs during a reload, an error will be triggered and handled in the same fashion as other reload errors.

If several files having the same `id` in that folder (for example `load-balancer.yml` and `load-balancer.json`), vector should error.

The pipeline's configuration files should only contain transforms or vector will error.
A pipeline cannot use another pipeline or vector will error.

The following wouldn't be possible, because the `generate` name would conflict.

```toml
# /etc/vector/first.toml
[sources.generate]
...
[resources.my-pipeline]
...
# /etc/vector/second.toml
[sources.generate]
...
[resources.my-pipeline]
...
# /etc/vector/pipelines/my-pipeline.yml
[transforms.something]
type = "remap"
inputs = ["generate"]
```

But it becomes possible with

```toml
# /etc/vector/first.toml
[sources.generate-first]
...
[resources.my-pipeline]
generate = ["generate-first"]
# /etc/vector/second.toml
[sources.generate-second]
...
[resources.my-pipeline]
generate = ["generate-second"]
# /etc/vector/pipelines/my-pipeline.yml
[transforms.something]
type = "remap"
inputs = ["generate"]
```

The pipeline resource would have the following structure

```rust
struct PipelineResource {
  filename: String,
  aliases: Map<String, Set<String>>,
  config: PipelineConfig,
}
```

The structure of the pipeline's configuration file is as follows

```toml
# optional
id = "load-balancer"

[transforms.first]
inputs = ["source"]
...

[transforms.second]
inputs = ["first"]
outputs = ["exposed-sink"]
...

# this will error
[transforms.third]
inputs = ["other-pipeline.bar"]
outputs = ["exposed-sink"]
...
```

In order to allow the user to specify the targeted sinks by a pipeline, the `forward` component creates a forwarding to the real sinks from the configuration files.
It also allows the user to use is own aliases and just refer the external sinks in a single place.

Now, if we look deeper at the configuration building process, the configuration compiler will require the pipelines in order to build the [configuration](https://github.com/timberio/vector/blob/v0.15.0/src/config/builder.rs#L71).

To do so, we'll need to implement a `PipelineConfigBuilder` with the following structures

```rust
struct PipelineTransformOuter {
    #[serde(flatten)]
    content: TransformOuter,
    outputs: Set<String>,
}
struct PipelineConfigBuilder {
    pub id: String,
    pub transforms: Map<String, PipelineTransformOuter>,
}
```

then we'll update [the `compile` function](https://github.com/timberio/vector/blob/v0.15.0/src/config/compiler.rs#L4), in order to build a `Config` containing the required pipeline components, the compiler will load the pipeline's configurations, load the transforms and substitute the aliases.

The components coming from the pipeline would be cloned inside the final `Config`, in the `transforms` `IndexMap` and the `outputs` from the pipeline components will be added to the referring `Sink` input field.

### Observing pipelines

Users should be able to observe and monitor individual pipelines.
This means relevant metrics coming from the `internal_metrics` source must contain a `pipeline_id` tag referring to the pipeline's `id`.

In Vector, the `Task` structure is what emits the events for `internal_metrics`.
After [build the different pieces of the topology](https://github.com/timberio/vector/blob/v0.15.0/src/topology/builder.rs#L106), we've to update the [`Task::new`](https://github.com/timberio/vector/blob/v0.15.0/src/topology/builder.rs#L163) in order to accept an `Option<PipelineId>` so that when it emits the metrics events it can provide the information about the pipeline.

This approach would extend the [RFC 2064](https://github.com/timberio/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md#collecting-uniform-context-data) by _just_ adding `pipeline_id` to the context.

When [spawning a transform](https://github.com/timberio/vector/blob/v0.15.0/src/topology/mod.rs#L574), adding the optional pipeline information to the span will populate the metrics.

```rust
let span = error_span!(
    "transform",
    component_kind = "transform",
    component_name = %task.name(),
    component_type = %task.typetag(),
    pipeline_id = %task.pipeline_id(),
);
```

## Rationale

- Why is this change worth it?

This split improves the readability of the configuration files and allows the users to collaborate, which makes using Vector more user friendly.

- What is the impact of not doing this?

This would force the users to keep having complex configuration files and/or to duplicate components configuration between their configuration files.

- How does this position us for success in the future?


## Prior Art

_TODO_

## Drawbacks

- Why should we not do this?
- What kind on ongoing burden does this place on the team?

## Alternatives

- Do nothing: we can already use several configuration files, people could split their existing configuration.

This would imply some duplication if a transform is used in multiple configuration files.
Anybody could add a sink or source even if they don't have permission.

- Do nothing: write a tool that would write a big configuration file where each pipeline would start with a dummy filter that we could monitor in the `internal_metrics`.



- Evolve vector to use a tag/filter model like our competitors, have a 'pipeline' be a 'tag'.

Not able to add internal metrics to specific transforms and monitor them.
Doesn't block to create other sources/sinks.

- Run a single vector per-'pipeline' and support metric tagging to distinguish at the telemetry level.

Adds lot of complexity and would add some constraints regarding resources that can only been used once.
Doesn't block to create other sources/sinks.

## Outstanding Questions

- Should the `pipelines` directory location be configurable through the cli?

```bash
vector --pipeline-dir /foo/bar/pipelines
```

- Should `pipelines` enable reuse? Should we be able to use it several times across a configuration?
- Should the `ops` have to do anything for a pipeline to load? Should we reference a pipeline in the configuration?
- Should the `devs` write a sort of function or a snippet (as a pipeline) that the `ops` would use?

## Plan Of Attack

- [ ] Add pipeline resource structure
- [ ] Create the Pipeline structure and parse a pipeline's configuration file
- [ ] Update topology with pipeline's components
- [ ] Update the context for taking pipeline informations

