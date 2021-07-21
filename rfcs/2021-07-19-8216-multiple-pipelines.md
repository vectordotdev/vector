# RFC 8216 - 2021-07-19 - multiple pipelines

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

To keep the actual structure and avoid retro compatibility issues, vector should read from the default folder `/etc/vector/pipelines/*` the difference pipeline's configuration files.
The pipeline directory should have a flat structure (no sub directories) and only `json`, `toml` and `yaml` files will be supported.

To discuss: make this folder should be configurable through the cli, the same way than for the configuration file, with `--pipelines-dir` or though and environment variable `VECTOR_PIPELINES_DIR`.

### How vector reads the pipelines and what are the limits

Vector will read each of the pipeline configuration files at start time and create an `id` attribute corresponding to the pipeline's filename. A `load-balancer.yml` will have `load-balancer` as `id`. In the pipeline's configuration file, the `id` can be set manually.
In addition, a `version` attribute will be set containing the hash of the pipeline's configuration file to keep track of the file changes.
In case several files having the same `id` in that folder (for example `load-balancer.yml` and `load-balancer.json`), vector should error.

Those pipeline's configuration files should only contain transforms or vector will error.
Each of the defined transforms will be callable by using `pipeline-id.transform-id` outside of the pipeline's configuration file to avoid conflicts.
A pipeline cannot use a component defined in another pipeline of vector will error.

The structure of the pipeline's configuration file should be as follow

```toml
id = "load-balancer"

[transforms.first]
...

[transforms.second]
...
```

Now, if we look deeper at the configuration building process, the configuration compiler will require the pipelines in order to build the [configuration](https://github.com/timberio/vector/blob/v0.15.0/src/config/builder.rs#L71).
To do so, we'll need to implement a `PipelineBuilder` that would work the same way as the the `ConfigBuilder` and use it in [the `compile` function](https://github.com/timberio/vector/blob/v0.15.0/src/config/compiler.rs#L4) in order to load the pipeline's components inside the configuration. When building the `Config`, the [transforms](https://github.com/timberio/vector/blob/v0.15.0/src/config/compiler.rs#L34) from the pipelines will have to be injected only when they are being used.

### What metadatas vector adds to the events going through a pipeline

Each time an event goes through a pipeline, vector will create a field (a tag for metrics) `pipelines.${PIPELINE_ID} = ${PIPELINE_VERSION}` in order to be able to track the path taken by an event.

To do so, here are 2 options.

#### By dynamically creating a transform each time a pipeline is used

This step of adding a field should be done with a dynamically created transform and injected in the configuration by the [compiler](https://github.com/timberio/vector/blob/v0.15.0/src/config/compiler.rs#L34).
Each time a component calls a transform coming from a pipeline, the inputs should be swapped and a dynamic transform created.

```toml
# in the pipeline load-balancer
[transforms.do-something]
...

# in the config
[sink.output]
input = ["load-balancer.do-something"]
...
```

should be replaced by this equivalent

```
# in the pipeline load-balancer
[transforms.do-something]
...

# injected transform
[transforms.pre-output]
input = ["load-balancer.do-something"]
type = "remap"
source = '''
if is_object(.pipelines) {
  .pipelines = merge(.pipelines, {"load-balancer":"version"})
} else {
  .pipelines = {"load-balancer":"version"}
}
'''

[sink.output]
input = ["pre-output"]
...
...
```

To avoid name conflicts with that dynamic transform, this generated transform name could be made by doing a hash of `pre-${NAME}(${INPUTS})`.

#### By adding the field each time an event goes through a pipeline

Adding the ability to a transform to be aware of its name and if it's part of a pipeline by updating `FunctionTransform` and `TaskTransform`:

```rust
pub trait FunctionTransform: ... {
  // returning the name and the version
  fn pipeline(&self) -> Option<(String, String)>;
  // name of the transform
  fn name(&self) -> String;
}
```

And then adding a `pre_transform` function that would add the pipeline fields

```rust
pub trait FunctionTransform: ... {
  fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
    self.do_transform(output, self.pre_transform);
  }

  fn pre_transform(&mut self, event: Event) -> Event {
    // update the event in order to add the field
  }

  fn transform(&mut self, output: &mut Vec<Event>, event: Event);
}
```

This solution would required the core of vector and how it handles transforms but would avoid generatic dynamic transforms.
It would probably also reduce the performance considering that each transform would have to check if it's part of a pipeline.

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

This improves the readability of the `vector.toml` and add the possibility to share pipelines or deploy them with some tools like Terraform or Ansible without modifying the main configuration.
In the future, this will allow us to replicate datadog's pipelines locally.

## Drawbacks

- Not yet able to use a pipeline in another pipeline, but this may come in further work.
- First option: creating some hidden transforms and modifying the existing ones could introduce some bugs.

## Alternatives

- Updating the `Topology` to create those transforms at the topology building time in order to avoid modifying the inputs when building the configuration.

## Outstanding Questions

- Should we have some visibility attributes on the components in different pipelines in order to make them available to other components?
- Should we have a main (or default) transform in a pipeline that would allow to use the pipeline's ID as a transform name?

## Plan Of Attack

1. Update the CLI to take the pipeline directory parameter
2. Create the Pipeline structure and parse a pipeline's configuration file

3. First option: Dynamically create transform that add the pipeline's metadata to every events
4. First option: Register the pipelines' components into vector

3. Second option: Update the transforms to take the transform name and pipeline info when being built
4. Second option: Set the events field when in a pipeline
