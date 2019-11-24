---
delivery_guarantee: "best_effort"
event_types: ["log"]
issues_url: https://github.com/timberio/vector/issues?q=is%3Aopen+is%3Aissue+label%3A%22source%3A+kubernetes%22
operating_systems: ["linux","macos","windows"]
sidebar_label: "kubernetes|[\"log\"]"
source_url: https://github.com/timberio/vector/tree/master/src/sources/kubernetes.rs
status: "beta"
title: "kubernetes source"
unsupported_operating_systems: []
---

The `kubernetes` source ingests data through kubernetes node's and outputs [`log`][docs.data-model#log] events.

## Configuration

import CodeHeader from '@site/src/components/CodeHeader';

<CodeHeader fileName="vector.toml" learnMoreUrl="/docs/setup/configuration"/ >

```toml
[sources.my_source_id]
  type = "kubernetes" # example, must be: "kubernetes"
```

## Options

import Fields from '@site/src/components/Fields';

import Field from '@site/src/components/Field';

<Fields filters={true}>


</Fields>

## Output

This component outputs [`log` events][docs.data-model.log].
For example:

```javascript
{
  "container_name": "vector",
  "host": "vector-agent-rmqbn",
  "pod_uid": "vector-f8dd5f7b-tvgfn_52cdc270-c3e6-4769-b0a9-275481502618",
  "stream": "stdout",
  "timestamp": "2019-11-01T21:15:47+00:00"
}
```
More detail on the output schema is below.

<Fields filters={true}>


<Field
  enumValues={null}
  examples={["vector"]}
  name={"container_name"}
  path={null}
  required={true}
  type={"string"}
  >

### container_name

The container name that Vector is running in. See [Metadata](#metadata) for more info.


</Field>


<Field
  enumValues={null}
  examples={["vector-agent-rmqbn"]}
  name={"host"}
  path={null}
  required={true}
  type={"string"}
  >

### host

The current hostname where of the local pod Vector is running in. See [Metadata](#metadata) for more info.


</Field>


<Field
  enumValues={null}
  examples={["vector-f8dd5f7b-tvgfn_52cdc270-c3e6-4769-b0a9-275481502618"]}
  name={"pod_uid"}
  path={null}
  required={true}
  type={"string"}
  >

### pod_uid

The pod UID that Vector is running in. See [Metadata](#metadata) for more info.


</Field>


<Field
  enumValues={null}
  examples={["stdout"]}
  name={"stream"}
  path={null}
  required={true}
  type={"string"}
  >

### stream

The [standard stream][urls.standard_streams] that the log was collected from. See [Metadata](#metadata) for more info.


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

The exact time the event was ingested. See [Metadata](#metadata) for more info.


</Field>


</Fields>

## How It Works

### Deployment

The `kubernetes` source is designed to be deployed to a Kubernetes cluster as a
[`DaemonSet`][urls.kubernetes_daemonset]. You can find an [example config] in the
`vector` repository. At a high level the `kubernetes` source will run an agent on
each node and will internally use the file source to collect logs from `/var/log/pods`
and a few other locations that Kubernetes places logs into. Via the [`DaemonSet`][urls.kubernetes_daemonset]
Kubernetes will ensure that there is always a copy of the agent running on each node.

### Environment Variables

Environment variables are supported through all of Vector's configuration.
Simply add `${MY_ENV_VAR}` in your Vector configuration file and the variable
will be replaced before being evaluated.

You can learn more in the [Environment Variables][docs.configuration#environment-variables]
section.

### Metadata

Each event will contain a `message` field that contains the direct output from the containers stdout/stderr. There
will also be other fields included that come from Kubernetes. These fields include[`host`](#host),[`stream`](#stream),[`pod_uid`](#pod_uid),[`container_name`](#container_name) and[`timestamp`](#timestamp).


[docs.configuration#environment-variables]: /docs/setup/configuration#environment-variables
[docs.data-model#log]: /docs/about/data-model#log
[docs.data-model.log]: /docs/about/data-model/log
[urls.kubernetes_daemonset]: https://kubernetes.io/docs/concepts/workloads/controllers/daemonset/
[urls.standard_streams]: https://en.wikipedia.org/wiki/Standard_streams
