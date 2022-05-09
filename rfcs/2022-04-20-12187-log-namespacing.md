# RFC 12187 - 2022-04-20 - Log Namespacing

Today, when deserializing data on an event the keys are arbitrary and can potentially collide with data on the root.
Not only is the loss of data inconvenient, but it prevents us from fully utilizing the power of schemas.
The event data should be restructured to prevent collisions, and in general make them easier to use.

## Goals


1. The schema / type should be known in all cases when using the "Vector" namespace
2. The "Global Log Schema" should be ignored
3. Users can opt in to the new namespace on a per-source basis (as well as globally)

## Proposal

### Changes

Support will be added for different log namespaces in Vector. The default will be a "Legacy" namespace, which keeps
behavior the same as it was before namespaces were introduced. A new "Vector" namespace will be added with the changes
described below. The goal is to allow an opt-in migration to the new namespace. Eventually the legacy namespace
can be deprecated and removed.

The user-facing configuration for setting the log namespace will start with a simple `log_namespace` boolean setting.
This will be available as a global setting and a per-source setting, both defaulting to false.
A value of false means the "Legacy" namespace is used, true means the "Vector" namespace is used.
This will seem like you are just enabling / disabling the "log namespace" feature. However, it leaves
the option open in the future to allow string values and pick the name of the namespace if more namespaces are added.

The "Global Log Schema" will be ignored when the Vector namespace is used. Instead of user-configurable keys, static
keys will be used. Previously this was useful so users could choose names that didn't collide with other data in the event.
With namespacing, this is no longer a concern.

Similar to the Global Log Schema, some sources allow choosing key names where some data will be stored. This will also be
removed when the "Vector" namespace is being used, in favor of static names (usually what the default was). An example
is the "key_field" and "headers_key" in the kafka source.

Many transforms / sinks rely on the "Global Log Schema" to get/modify info such as the timestamp. Since the log
schema should be customizable per source, that means the transforms and sinks need to know which log namespace
is being used by each log. In order to accomplish this, the `log_namespace` event metadata field will be set to
`vector` if the new namespace is being used.



### Types of Data

#### Data

This is the main content of a log.  It will be placed at the root of the event.
What is considered the "main content" here is not very well-defined, but should generally be what most people would expect
to receive from the source. It is not necessarily the "log message" itself.

A good example is the "Datadog Agent" source. The root is the fields received by the Datadog agent. The log content is
nested.

Others such as the "socket" source will have the log content directly at the root.

There is a special case when the codec is "bytes", since the data is just a string in that case. That means
that the root is a string. Historically this has been forbidden, but it is possible to allow this.
An example of this is the "socket" source, with the "bytes" codec.


#### Metadata

This is any useful data that is not placed on the event. This will be stored in event metadata. Some improvements will be needed to
event metadata to make this easier to use, as described below. This currently contains fields such as `datadog_api_key`.
Other "vector" metadata such as `ingest_timestamp` and `source_type` will be added here, as well as additional metadata from
sources, such as `key` and `headers` from the `kafka` source.

### Codecs

#### Bytes

With the "Legacy" namespace, this codec decodes the input as-is, and places it under a "message" key (configurable with global log schema).
With the "Vector" namespace, this will be placed under the "data" key. If there is no additional metadata stored
on the event, then it will be placed on the root instead. The placement will be decided for each source independently.

#### Json

With the "Legacy" namespace, this codec decodes the input as-is, and places it under a "message" key (configurable with global log schema).
With the "Vector" namespace, this will be placed under the "data" key. If there is no additional metadata stored
on the event, then it will be placed on the root instead. The placement will be decided for each source independently.

#### Syslog

All data will be placed at the root. Since the structure is known, it's easy to prevent naming collisions here.
Syslog has a "message" field which might be useful to apply an additional codec to, but this is currently not supported,
and no support is being added here.

#### Native / Native JSON

When these are used in the "Vector" source, the behavior will not change, and events will be passed through as-is.

### Event Metadata

There are 3 sources of information that will be stored in Event Metadata.
1. Vector "internal" metadata such as `datadog_api_key`, so it can be read/set by users. This data already exists.
2. Vector metadata such as `ingest_timestamp`, and `source_type` which are set for every source.
3. Source metadata which will vary for each source.

There is currently minimal support for event metadata. Several changes will need to be made.

Changes needed immediately:

- Support arbitrarily nested values
- Functions that access metadata (`get_metadata_field`, `remove_metadata_field`, and `set_metadata_field`) should support full paths as keys, and return
  the `any` type instead of just `string`. This is technically a breaking change.

With these changes, using metadata can still be a bit annoying since the returned type will always be `any`, even if the
value is set and read in the same VRL program.

Future changes (out of scope for now):

- Add a special path syntax to reference metadata instead of using functions.
- Expand schema support to metadata
- Expand semantic meaning to metadata


### Implementation

A proof of concept for the Datadog Agent Logs source is being worked on alongside this RFC: [12218](https://github.com/vectordotdev/vector/pull/12218)


### Examples

All examples shown are using the new Vector namespace

#### Datadog Agent source / JSON codec

event

```json
{
  "message": {
    "derivative": -2.266778047142367e+125,
    "integral": "13028769352377685187",
    "mineral": "H 9 ",
    "proportional": 3673342615,
    "vegetable": -30083
  },
  "ddsource": "waters",
  "ddtags": "env:prod",
  "hostname": "beta",
  "service": "cernan",
  "status": "notice",
  "timestamp": "2066-08-09T04:24:42.1234Z" // This is parsed from a unix timestamp provided by the DD agent
}
```

metadata

```json
{
  "log_namespace": "vector",
  "source_type": "datadog_agent",
  "ingest_timestamp": "2022-04-14T19:14:21.899623781Z",

  // These are existing fields in event metadata that may be on any event
  "datadog_api_key": "2o86gyhufa2ugyf4",
  "splunk_hec_token": "386ygfhawnfud6rjftg"
}
```

-----

#### Datadog Agent source / bytes codec

event

```json
{
  "message": "{\"proportional\":702036423,\"integral\":15089925750456892008,\"derivative\":-6.4676193438086e263,\"vegetable\":20003,\"mineral\":\"vsd5fwYBv\"}",
  "ddsource": "waters",
  "ddtags": "env:prod",
  "hostname": "beta",
  "service": "cernan",
  "status": "notice",
  "timestamp": "2066-08-09T04:24:42.1234Z"
}
```

metadata

```json
{
  "log_namespace": "vector",
  "source_type": "datadog_agent",
  "ingest_timestamp": "2022-04-14T19:14:21.899623781Z"
}
```

-----

#### Kafka source / json codec

event

```json
{
  "derivative": -2.266778047142367e+125,
  "integral": "13028769352377685187",
  "mineral": "H 9 ",
  "proportional": 3673342615,
  "vegetable": -30083

}
```

metadata

```json
{
  "key": "the key of the message"
  // headers were originally nested under a configurable "headers_key". This is using a static value.
  "headers": {
    "header-a-key": "header-a-value",
    "header-b-key": "header-b-value",
  }
  "log_namespace": "vector",
  "topic": "name of topic",
  "partition": 3,
  "offset": 1829448,
  "source_type": "kafka",
  "ingest_timestamp": "2022-04-14T19:14:21.899623781Z"
}
```

-----

#### Kubernetes Logs / Vector namespace

event (a string as the root event element)

```text
F1015 11:01:46.499073       1 main.go:39] error getting server version: Get \"https://10.96.0.1:443/version?timeout=32s\": dial tcp 10.96.0.1:443: connect: network is unreachable
```

metadata

```json
{
  "log_namespace": "vector",
  "file": "/var/log/pods/kube-system_storage-provisioner_93bde4d0-9731-4785-a80e-cd27ba8ad7c2/storage-provisioner/1.log",
  "container_image": "gcr.io/k8s-minikube/storage-provisioner:v3",
  "container_name": "storage-provisioner",
  "namespace_labels": {
    "kubernetes.io/metadata.name": "kube-system"
  },
  "pod_annotations": {
    "prometheus.io/scrape": "false"
  },
  "pod_ip": "192.168.1.1",
  "pod_ips": [
    "192.168.1.1",
    "::1"
  ],
  "pod_labels": {
    "addonmanager.kubernetes.io/mode": "Reconcile",
    "gcp-auth-skip-secret": "true",
    "integration-test": "storage-provisioner"
  },
  "pod_name": "storage-provisioner",
  "pod_namespace": "kube-system",
  "pod_node_name": "minikube",
  "pod_uid": "93bde4d0-9731-4785-a80e-cd27ba8ad7c2",
  "stream": "stderr",
  "source_type": "kubernetes_logs",
  "ingest_timestamp": "2020-10-15T11:01:46.499555308Z"
}
```

-----


#### Syslog / Vector namespace

event (the "data" key was elided)

```json
{
    "message": "Hello Vector",
    "hostname": "localhost",
    "severity": "info",
    "facility": "facility",
    "appname": "Vector Hello World",
    "msgid": "238467-435-235-a3478fh",
    "procid": 13512,
    // this name is up for debate. Arbitrary keys need to be nested under something though
    "structured_data": {
      "origin": "timber.io"
    }
}
```

metadata

```json
{
  "log_namespace": "vector",
  "source_ip": "127.0.0.1",
  "hostname": "localhost",
  "source_type": "syslog",
  "ingest_timestamp": "2020-10-15T11:01:46.499555308Z"
}
```

-----

#### Socket source (mode=udp) / syslog codec / Vector namespace

event (the "data" key was elided)

```json
{
    "message": "Hello Vector",
    "hostname": "localhost",
    "severity": "info",
    "facility": "facility",
    "appname": "Vector Hello World",
    "msgid": "238467-435-235-a3478fh",
    "procid": 13512
}
```

metadata

```json
{
  "log_namespace": "vector",
  "source_ip": "192.168.0.1",
  "hostname": "localhost",
  "source_type": "socket",
  "ingest_timestamp": "2020-10-15T11:01:46.499555308Z"
}
```

-----

#### HTTP source / JSON codec / Vector namespace

event

```json
{
  "mineral": "quartz",
  "food": "sushi"
}
```

metadata

```json
{
  "path": "/foo/bar",
  // headers and query params were previously placed directly on the root. This needs to be nested to avoid potential naming conflicts.
  "headers": {
    "Content-Type": "application/json"
  },
  "query_params": {
    "page": 14,
    "size": 3
  }
  "log_namespace": "vector",
  "source_type": "http",
  "ingest_timestamp": "2020-10-15T11:01:46.499555308Z"
}
```

-----

#### Kafka source / Native codec / Vector namespace

This is an example where an event came from a Kafka source (JSON codec), through a Kafka sink (using native codec), and then back out a Kafka source (native codec).
Notice that since `key` and `headers` wasn't moved into the event (from the event metadata), those values from the first kafka source were lost.

event

```json
{
  "derivative": -2.266778047142367e+125,
  "integral": "13028769352377685187",
  "mineral": "H 9 ",
  "proportional": 3673342615,
  "vegetable": -30083
}
```

metadata (only from the 2nd kafka source)

```json
{
  "key": "the key of the message (from the 2nd kafka source)"
  "headers": {
    "header-a-key": "header-a-value (from the 2nd kafka source)",
    "header-b-key": "header-b-value (from the 2nd kafka source)"
  }
  "log_namespace": "vector",
  "topic": "name of topic",
  "partition": 3,
  "offset": 1829448,
  "source_type": "kafka",
  "ingest_timestamp": "2022-04-14T19:14:21.899623781Z"
}
```

## Outstanding Questions / Alternatives

### Additional nesting in event metadata

There's now 3 sources of information for the event metadata. It might be confusing to users where the data is coming
from (Vector vs source). Additional nesting could make this more clear, at the expense of things being a bit more nested.


## Prior Art

- OpenTelemetry: https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/logs/data-model.md


## Plan Of Attack

- Complete the proof of concept PR for the Data Agent Logs source. ([12218](https://github.com/vectordotdev/vector/pull/12218))
- Add log namespace support to each source

