# RFC 12187 - 2022-04-20 - Log Namespacing

Today, when deserializing data on an event the keys are arbitrary and can potentially collide with data on the root.
Not only is the loss of data inconvenient, but it prevents us from fully utilizing the power of schemas.
The event data should be restructured to prevent collisions, and in general make them easier to use.


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


### Types of Data

#### Data

This is the main content of a log. The log message itself. This will be parsed according to the set codec.
The decoded data will be placed on the event with a "data" prefix. In cases where the source
does not have any metadata that will also be on the event, this may be placed at the root.

There is a special case when the codec is "bytes", and the data will be placed at the root. That means
that the root is a string. Historically this has been forbidden, but it is possible to allow this.
An example of this is the "socket" source, with the "bytes" codec.

#### Metadata

This is any additional data that is not considered the log message itself, but is important / part of the source protocol.
A good example is the "key" for kafka events. Kafka is key/value based, but only the value is decoded by the chosen codec.
The key is still very important information that will be used by the sink.

In general, if the data would be used by a sink when writing the data, it is considered "metadata" (stored on the event).
Otherwise, it iss "vector metadata" (stored on the event metadata).

#### Vector Metadata

This is anything that is not really part of the log, but describes something useful _about_ the log, such as where
it came from, or when it was received. These will be stored in event metadata. Some improvements will be needed to
event metadata to make this easier to use, as described below.

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

The behavior for these do not change. Sources such as the "Vector" source will pass through the event as-is.

### Event Metadata

There is currently minimal support for event metadata. Several changes will need to be made.

Changes needed immediately:

- Support arbitrarily nested values
- Functions that access metadata (`get_metadata_field`, `remove_metadata_field`, and `set_metadata_field`) should support full paths as keys, and return
  the `any` type instead of just `string. This is technically a breaking change.

With these changes, using metadata can still be a bit annoying since the returned type will always be `any`, even if the
value is set and read in the same VRL program.

Future changes (out of scope for now):

- Add a special path syntax to reference metadata instead of using functions.
- Expand schema support to metadata
- Expand semantic meaning to metadata


### Implementation

A proof of concept for the Datadog Agent Logs source is being worked on alongside this RFC: [12218](https://github.com/vectordotdev/vector/pull/12218)


### Examples

#### Datadog Agent source / JSON codec / Vector namespace

event

```json
{
  "data": {
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
  "source_type": "datadog_agent",
  "ingest_timestamp": "2022-04-14T19:14:21.899623781Z"
}
```

-----

#### Datadog Agent source / bytes codec / Vector namespace

event

```json
{
  "data": "{\"proportional\":702036423,\"integral\":15089925750456892008,\"derivative\":-6.4676193438086e263,\"vegetable\":20003,\"mineral\":\"vsd5fwYBv\"}",
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
  "source_type": "datadog_agent",
  "ingest_timestamp": "2022-04-14T19:14:21.899623781Z"
}
```

-----

#### Kafka source / json codec / Vector namespace

event

```json
{
  "data": {
    "derivative": -2.266778047142367e+125,
    "integral": "13028769352377685187",
    "mineral": "H 9 ",
    "proportional": 3673342615,
    "vegetable": -30083
  },
  "key": "the key of the message"
  // headers were originally nested under a configurable "headers_key". This is using a static value.
  "headers": {
    "header-a-key": "header-a-value",
    "header-b-key": "header-b-value",
  }
}
```

metadata

```json
{
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
    "data": {
      "mineral": "quartz",
      "food": "sushi"
    },
    "path": "/foo/bar",
    // headers and query params were previously placed directly on the root. This needs to be nested to avoid potential naming conflicts.
    "headers": {
      "Content-Type": "application/json"
    },
    "query_params": {
      "page": 14,
      "size": 3
    }
}
```

metadata

```json
{
  "source_type": "http",
  "ingest_timestamp": "2020-10-15T11:01:46.499555308Z"
}
```

-----



## Outstanding Questions / Alternatives

### Minimize data on the event

The split of which data belongs in the event vs event metadata can sometimes be ambiguous. An alternative is to store only the
"data" in the event, and everything else ("metadata" and "vector metadata") in the event metadata.
This will keep the event itself very clean and easy to access.

If there is extra data needed for a sink (such as the key for kafka), then the user will either need to use VRL
to move that into the event, or a special path syntax will need to be added to refer to data on metadata in a sink.

### Don't use Event Metadata

One of the reasons to move some data to the "event metadata" is to prevent "unnecessary" data from
being written to a sink that is just metadata.

Using event metadata is a significant amount of work, as well as cognitive burden for users to introduce
a new concept.

Vector has the concept of "semantic meaning" to assign explicit meaning to different parts of an event.
Today all data on the event is generally considered to be the "data" when writing to a sink, and by
storing data in the "event metadata", it is effectively preventing the sink from writing that by default.

Instead, everything could be stored in the event (under appropriate namespacing to prevent key collisions).
Semantic meaning can be used to determine where the actual "data" lives. By default, data that would have been
stored in "event metadata" would be ignored by sinks, based on the semantic meaning automatically applied.

This should be able to achieve the same thing as using "event metadata". The main downside is that it
requires semantic meaning to be fully implemented and enabled, which is not done yet.

### Strings as a root event field

This RFC proposes allowing a string as the root event element in some cases. This has historically not been
allowed, and many parts of the code will currently panic in such a situation, although it's certainly possible
to allow this. The alternative is to always nest this under "data" instead.


### Namespace names

Right now the decoded "data" is placed under the prefix "data". Other names might make more sense, such as
`message`, `body`, `payload`, etc. It might also make sense to choose different names depending on each source.
For example, it might be better for the `http` source to use `body` but the `kafka` source to use `message`.

## Prior Art

- OpenTelemetry: https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/logs/data-model.md


## Plan Of Attack

- Complete the proof of concept PR for the Data Agent Logs source. ([12218](https://github.com/vectordotdev/vector/pull/12218))
- Add log namespace support to each source

