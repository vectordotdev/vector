---
event_types: ["log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+docker%22
output_types: ["log"]
sidebar_label: "docker|[\"log\"]"
source_url: https://github.com/timberio/vector/tree/master/src/sources/docker.rs
status: "beta"
title: "docker source" 
---

The `docker` source ingests data through the docker engine daemon and outputs [`log`][docs.data-model.log] events.

## Configuration

import CodeHeader from '@site/src/components/CodeHeader';
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

<Tabs
  defaultValue="common"
  values={[
    { label: 'Common', value: 'common', },
    { label: 'Advanced', value: 'advanced', },
  ]
}>
<TabItem value="common">

<CodeHeader fileName="vector.toml" learnMoreUrl="/usage/configuration"/ >

```toml
[sources.my_source_id]
  type = "docker" # example, must be: "docker"
```

</TabItem>
<TabItem value="advanced">

<CodeHeader fileName="vector.toml" learnMoreUrl="/usage/configuration" />

```toml
[sources.my_source_id]
  # REQUIRED
  type = "docker" # example, must be: "docker"
  
  # OPTIONAL
  include_containers = "ffd2bc2cb74a" # example, no default
  include_labels = "label_key=label_value" # example, no default
```

</TabItem>

</Tabs>

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={["ffd2bc2cb74a"]}
  name={"include_containers"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  type={"[string]"}
  unit={null}>

### include_containers

A list of container ids to match against when filtering running containers. This will attempt to match the container id from the beginning meaning you do not need to include the whole id but just the first few characters. If no containers ids are provided, all containers will be included.


</Option>


<Option
  common={false}
  defaultValue={null}
  enumValues={null}
  examples={["label_key=label_value"]}
  name={"include_labels"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  type={"[string]"}
  unit={null}>

### include_labels

 A list of container object labels to match against when filtering running containers. This should follow the described label's synatx in [docker object labels docs][urls.docker_object_labels]. 


</Option>


</Options>

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

## Guides


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[urls.docker_daemon]: https://docs.docker.com/engine/reference/commandline/dockerd/#daemon-socket-option
[urls.docker_object_labels]: https://docs.docker.com/config/labels-custom-metadata/
