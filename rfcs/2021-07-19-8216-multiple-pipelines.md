# RFC 8216 - 2021-07-19 - multiple pipelines

Large Vector users often require complex Vector topologies to facilitate the collection and processing of data from many different upstream sources. This results in large Vector configuration files that are hard to manage across different teams. This makes Vector a bad candidate for large teams that require autonomy to facilitate sane collaboration.

## Scope

This RFC will cover:

- How and where the pipelines are stored
- How vector reads the pipelines and what are the limits of pipelines
- What metadatas vector adds to the events going through a pipeline

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
This folder should be configurable through the cli, the same way than for the configuration file, with `--pipelines-dir` or though and environment variable `VECTOR_PIPELINES_DIR`.
The pipeline directory should have a flat structure (no sub directories) and only `json`, `toml` and `yaml` files will be supported.

### How vector reads the pipelines and what are the limits

Vector will read each of the pipeline configuration files at start time and create an `id` and a `name` attribute corresponding to the pipeline's filename. A `load-balancer.yml` will have `load-balancer` as `id` and `name`. In the pipeline's configuration file, the `id` can be set manually.
In addition, a `version` attribute will be set containing the hash of the pipeline's configuration file to keep track of the file changes.
In case several files having the same `id` or `name` in that folder (for example `load-balancer.yml` and `load-balancer.json`), vector should error.

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

### What metadatas vector adds to the events going through a pipeline

Each time an event goes through a pipeline, vector will create a field (a tag for metrics) `pipelines.${PIPELINE_ID} = ${PIPELINE_VERSION}` in order to be able to track the path taken by an event.

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

## Prior Art

`TODO`

## Drawbacks

- Not yet able to use a pipeline in another pipeline, but this may come in further work.

## Alternatives

`TODO`

## Outstanding Questions

- Should we have some visibility attributes on the transforms in different pipelines in order to make them available to other components?
- Should we have a main (or default) transform in a pipeline that would allow to use the pipeline's ID as a transform name?

## Plan Of Attack

1. Update the CLI to take the pipeline directory parameter
2. Create the Pipeline structure and parse a pipeline's configuration file
3. Dynamically create transform that add the pipeline's metadata to every events
4. Register the pipelines' transforms into vector

