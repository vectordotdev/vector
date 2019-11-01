---
title: "docker source" 
sidebar_label: "docker"
---

The `docker` source ingests data through the docker engine daemon and outputs [`log`][docs.data-model.log] events.

## Example

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

<Tabs
  defaultValue="simple"
  values={[
    { label: 'Simple', value: 'simple', },
    { label: 'Advanced', value: 'advanced', },
  ]
}>
<TabItem value="simple">

```coffeescript
[sources.my_source_id]
  type = "docker" # enum
```

</TabItem>
<TabItem value="advanced">

```coffeescript
[sources.my_source_id]
  # REQUIRED
  type = "docker" # enum
  
  # OPTIONAL
  include_containers = "ffd2bc2cb74a" # no default
  include_labels = "key=value" # no default
```

</TabItem>

</Tabs>

You can learn more

## Options

import Option from '@site/src/components/Option';
import Options from '@site/src/components/Options';

<Options filters={true}>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["ffd2bc2cb74a"]}
  name={"include_containers"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
  type={"[string]"}
  unit={null}>

### include_containers

A list of container ids to match against when filtering running containers. This will attempt to match the container id from the beginning meaning you do not need to include the whole id but just the first few characters. If no containers ids are provided, all containers will be included.


</Option>


<Option
  defaultValue={null}
  enumValues={null}
  examples={["key=value"]}
  name={"include_labels"}
  nullable={true}
  path={null}
  relevantWhen={null}
  required={false}
  simple={false}
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

### Delivery Guarantee

Due to the nature of this component, it offers a
[**best effort** delivery guarantee][docs.guarantees#best-effort-delivery].

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

## Troubleshooting

The best place to start with troubleshooting is to check the
[Vector logs][docs.monitoring#logs]. This is typically located at
`/var/log/vector.log`, then proceed to follow the
[Troubleshooting Guide][docs.troubleshooting].

If the [Troubleshooting Guide][docs.troubleshooting] does not resolve your
issue, please:

1. Check for any [open `docker_source` issues][urls.docker_source_issues].
2. If encountered a bug, please [file a bug report][urls.new_docker_source_bug].
3. If encountered a missing feature, please [file a feature request][urls.new_docker_source_enhancement].
4. If you need help, [join our chat/forum community][urls.vector_chat]. You can post a question and search previous questions.

## Resources

* [**Issues**][urls.docker_source_issues] - [enhancements][urls.docker_source_enhancements] - [bugs][urls.docker_source_bugs]
* [**Source code**][urls.docker_source_source]


[docs.configuration#environment-variables]: ../../../usage/configuration#environment-variables
[docs.data-model.log]: ../../../about/data-model/log.md
[docs.guarantees#best-effort-delivery]: ../../../about/guarantees.md#best-effort-delivery
[docs.monitoring#logs]: ../../../usage/administration/monitoring.md#logs
[docs.troubleshooting]: ../../../usage/guides/troubleshooting.md
[urls.docker_daemon]: https://docs.docker.com/engine/reference/commandline/dockerd/#daemon-socket-option
[urls.docker_object_labels]: https://docs.docker.com/config/labels-custom-metadata/
[urls.docker_source_bugs]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+docker%22+label%3A%22Type%3A+bug%22
[urls.docker_source_enhancements]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+docker%22+label%3A%22Type%3A+enhancement%22
[urls.docker_source_issues]: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+docker%22
[urls.docker_source_source]: https://github.com/timberio/vector/tree/master/src/sources/docker.rs
[urls.new_docker_source_bug]: https://github.com/timberio/vector/issues/new?labels=source%3A+docker&labels=Type%3A+bug
[urls.new_docker_source_enhancement]: https://github.com/timberio/vector/issues/new?labels=source%3A+docker&labels=Type%3A+enhancement
[urls.vector_chat]: https://chat.vector.dev
