# RFC 8216 - 2021-07-19 - Multiple Pipelines

Large Vector users often require complex Vector topologies to facilitate the collection and processing of data from many different upstream sources. This results in large Vector configuration files that are hard to manage across different teams. This makes Vector a bad candidate for large teams that require autonomy to facilitate sane collaboration.

## Scope

This RFC will cover:

- How and where the pipelines are stored
- How vector reads the pipelines and what are the limits of pipelines
- What metadata vector adds to the events going through a pipeline

This RFC will not cover:

- How the pipelines should be synchronized between vector instances
- Pre/post processing of data - The ability to prepare and normalize data.
- Connecting pipelines together - The ability to take input from another pipeline.
- Component reuse - The ability to define boilerplate for reuse across many different pipelines. This will likely align with Datadog’s “pipeline catalogue”.
- Pipeline quotas - The ability to limit how much data a pipeline can send to a sink.


## Motivation

- Helps Vector to grow organically within an organization by allowing teams to adopt Vector at their own pace without heavy administrator involvement.
- Reduces the management overhead of devops/SREs by enabling teams to manage their own pipelines (spread the management load).

## Internal Proposal

### How and where the pipelines are stored

To avoid backward incompatibility, pipelines will be loaded from a `pipelines` sub-directory relative to the Vector configuration directory (e.g., `/etc/vector/pipelines`). Therefore, if a user changes the location of the Vector configuration directory they will also change the `pipelines` directory path. They are coupled.
The `pipelines` directory will contain all pipelines represented as individual files. For simplicity, and to ensure users do not over complicate pipeline management, sub-directories/nesting are not allowed. This is inspired by Terraform's single-level directory nesting, which has been a net positive for simple management of large Terraform projects.
Each pipeline file is a processing subset of a larger Vector configuration file. Therefore, it follows the same syntax as Vector's configuration (`toml`, `yaml`, and `json`).


### How vector reads the pipelines and what are the limits

Each time Vector will read its configuration file (on boot and when it live reloads), it will read the pipeline configuration files beforehand and associate an `id` attribute corresponding to the pipeline's filename.
A `load-balancer.yml` will have `load-balancer` as `id`. In the pipeline's configuration file, the `id` can be set manually to override this behavior.
In addition, a `version` attribute will be set containing the hash of the pipeline's configuration file to keep track of the file changes.
In case several files having the same `id` in that folder (for example `load-balancer.yml` and `load-balancer.json`), vector should error.

Those pipeline's configuration files should only contain transforms or vector will error.
Each of the components in the pipeline configuration can be refered to with the `id` of the pipeline and the component `id` (for example, the transform `foo` in the pipeline `bar` should be set as an input with `foo.bar`).
A pipeline cannot use a component defined in another pipeline or vector will error.

The structure of the pipeline's configuration file should be as follows

```toml
# optional
id = "load-balancer"

[transforms.first]
inputs = ["source"]
...

[transforms.second]
inputs = ["load-balancer.first"]
...

# this will error
[transforms.third]
inputs = ["other-pipeline.bar"]
...
```

Now, if we look deeper at the configuration building process, the configuration compiler will require the pipelines in order to build the [configuration](https://github.com/timberio/vector/blob/v0.15.0/src/config/builder.rs#L71).

To do so, we'll need to implement a `PipelineBuilder` with the following structure

```rust
struct PipelineBuilder {
    pub id: String,
    pub version: String,
    pub transforms: IndexMap<String, TransformOuter>,
}
```

then we would update [the `compile` function](https://github.com/timberio/vector/blob/v0.15.0/src/config/compiler.rs#L4)

```rust
fn compile(mut builder: ConfigBuilder, pipelines: Vec<PipelineBuilder>)
```

in order to build a `Config` containing the required pipeline components.

The components coming from the pipeline would be cloned inside the final `Config`, in the `transforms` `IndexMap`.


### Observing pipelines

Users should be able to observe and monitor individual pipelines.
This means relevant metrics coming from the `internal_metrics` source must contain a `pipeline_id` tag refering to the pipeline `id` and a `pipeline_version` tag refering to the pipeline `version`.

In Vector, the `Task` structure is what emits the events for `internal_metrics`.
After [build the different pieces of the topology](https://github.com/timberio/vector/blob/v0.15.0/src/topology/builder.rs#L106), we've to update the [`Task::new`](https://github.com/timberio/vector/blob/v0.15.0/src/topology/builder.rs#L163) in order to accept an `Option<(PipelineId, PipelineVersion)>` so that when it emits the metrics events it can provide the information about the pipeline.

This approach would extend the [RFC 2064](https://github.com/timberio/vector/blob/master/rfcs/2020-03-17-2064-event-driven-observability.md#collecting-uniform-context-data) by _just_ adding `pipeline_id` and `pipeline_version` to the context.

When [spawning a transform](https://github.com/timberio/vector/blob/v0.15.0/src/topology/mod.rs#L574), adding the optional pipeline information to the span will populate the metrics.

```rust
let span = error_span!(
    "transform",
    component_kind = "transform",
    component_name = %task.name(),
    component_type = %task.typetag(),
    pipeline_id = %task.pipeline_id(),
    pipeline_version = %task.pipeline_version(),
);
```

## Doc-level Proposal

With this pipeline definition, the following `vector.toml`

```toml
[sources.docker]
type = "docker_logs"
...

[transforms.nginx_filter]
input = ["docker"]
type = "remap"
source = '''
  # do something that filter nginx logs from docker logs
'''

[transforms.nginx]
input = ["nginx_filter"]
type = "remap"
source = '''
  # do some transformations with the nginx logs
'''

[transforms.apache_filter]
input = ["docker"]
type = "remap"
source = '''
  # do something that filter apache logs from docker logs
'''

[transforms.apache]
input = ["apache_filter"]
type = "remap"
source = '''
  # do some transformations with the apache logs
'''

[sinks.ouput]
inputs = ["apache", "nginx"]
type = "console"
...
```

can be split into the following `pipelines/frontend.toml`

```toml
[transforms.nginx_filter]
input = ["docker"]
type = "remap"
source = '''
  # do something that filter nginx logs from docker logs
'''

[transforms.nginx]
input = ["frontend.nginx_filter"]
type = "remap"
source = '''
  # do some transformations with the nginx logs
'''

[transforms.apache_filter]
input = ["docker"]
type = "remap"
source = '''
  # do something that filter apache logs from docker logs
'''

[transforms.apache]
input = ["frontend.apache_filter"]
type = "remap"
source = '''
  # do some transformations with the apache logs
'''
```

and `vector.toml`

```toml
[sources.docker]
type = "docker_logs"
...

[sinks.ouput]
inputs = ["frontend.apache", "frontend.nginx"]
type = "console"
...
```

## Rationale

- This improves the readability of the `vector.toml`.

## Drawbacks

_TODO_

## Alternatives

- Do nothing: we can already use several configuration files, people could split their existing configuration.

This would imply some duplication if a transform is used in multiple configuration files.

- Evolve vector to use a tag/filter model like our competitors, have a 'pipeline' be a 'tag'.


- Run a single vector per-'pipeline' and support metric tagging to distinguish at the telemetry level.


## Outstanding Questions

- Should the `pipelines` directory location be configurable through the cli?

```bash
vector --pipeline-dir /foo/bar/pipelines
```

- Should we have some visibility attributes on the components in different pipelines in order to make them available to other components?

```toml
id = "pipeline-id"

[transforms.name]
private = true
```

- Should we have a main (or default) transform in a pipeline that would allow to use the pipeline's ID as a transform name?

```toml
# in the pipeline
id = "pipeline-id"

[transforms.default]
...

# in the configuration file
[sinks.out]
inputs = ["pipeline-id"]
```

- Should we allow to embed a pipeline configuration in a configuration file?

```toml
# in /etc/vector/vector.toml
[[pipelines]]
id = "first"

[[pipelines]]
id = "second"
```

## Plan Of Attack

- [ ] Create the Pipeline structure and parse a pipeline's configuration file
- [ ] Update the context for taking pipeline informations

