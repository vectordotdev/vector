---
delivery_guarantee: "best_effort"
event_types: ["log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+docker%22
sidebar_label: "docker|[\"log\"]"
source_url: https://github.com/timberio/vector/tree/master/src/sources/docker.rs
status: "beta"
title: "docker source" 
---

The `docker` source ingests data through the docker engine daemon and outputs [`log`][docs.data-model#log] events.

## Configuration

import CodeHeader from '@site/src/components/CodeHeader';

<CodeHeader fileName="vector.toml" learnMoreUrl="/docs/setup/configuration"/ >

```toml
[sources.my_source_id]
  # REQUIRED
  type = "docker" # example, must be: "docker"
  
  # OPTIONAL
  include_containers = "ffd2bc2cb74a" # example, no default
  include_labels = "label_key=label_value" # example, no default
```

## Options

import Fields from '@site/src/components/Fields';

import Field from '@site/src/components/Field';

<Fields filters={true}>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["ffd2bc2cb74a"]}
  name={"include_containers"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"[string]"}
  unit={null}
  >

### include_containers

A list of container ids to match against when filtering running containers. This will attempt to match the container id from the beginning meaning you do not need to include the whole id but just the first few characters. If no containers ids are provided, all containers will be included.


</Field>


<Field
  common={true}
  defaultValue={null}
  enumValues={null}
  examples={["label_key=label_value"]}
  name={"include_labels"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  templateable={false}
  type={"[string]"}
  unit={null}
  >

### include_labels

 A list of container object labels to match against when filtering running containers. This should follow the described label's synatx in [docker object labels docs][urls.docker_object_labels]. 


</Field>


</Fields>

## Output

This component outputs [`log` events][docs.data-model.log].
For example:

```javascript
{
  "container": "evil_ptolemy",
  "message": "Started GET / for 127.0.0.1 at 2012-03-10 14:28:14 +0100",
  "stream": "stdout",
  "timestamp": "2019-11-01T21:15:47+00:00"
}
```
More detail on the output schema is below.

<Fields filters={true}>


<Field
  enumValues={null}
  examples={["evil_ptolemy"]}
  name={"container"}
  path={null}
  required={true}
  type={"string"}
  >

### container

The Docker container name that the log was collected from.


</Field>


<Field
  enumValues={null}
  examples={["Started GET / for 127.0.0.1 at 2012-03-10 14:28:14 +0100"]}
  name={"message"}
  path={null}
  required={true}
  type={"string"}
  >

### message

The raw log message, unaltered.



</Field>


<Field
  enumValues={["stdout","stderr"]}
  examples={["stdout","stderr"]}
  name={"stream"}
  path={null}
  required={true}
  type={"string"}
  >

### stream

The [standard stream][urls.standard_streams] that the log was collected from.


</Field>


<Field
  enumValues={null}
  examples={["2019-11-01T21:15:47+00:00"]}
  name={"timestamp"}
  path={null}
  required={true}
  type={"timestamp"}
  >

### timestamp

The timestamp extracted from the Docker log event.



</Field>


</Fields>

## How It Works

### Connecting to the Docker daemon

Vector will automatically attempt to connect to the docker daemon for you. In most
situations if your current user is able to run `docker ps` then Vector will be able to
connect. Vector will also respect if `DOCKER_HOST` and `DOCKER_VERIFY_TLS` are set. Vector will also
use the other default docker environment variables if they are set. See the [Docker daemon docs][urls.docker_daemon].
### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.


[docs.configuration#environment-variables]: /docs/setup/configuration#environment-variables
[docs.data-model#log]: /docs/about/data-model#log
[docs.data-model.log]: /docs/about/data-model/log
[urls.docker_daemon]: https://docs.docker.com/engine/reference/commandline/dockerd/#daemon-socket-option
[urls.docker_object_labels]: https://docs.docker.com/config/labels-custom-metadata/
[urls.standard_streams]: https://en.wikipedia.org/wiki/Standard_streams
