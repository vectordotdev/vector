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
is being used by each log. The existence of the read-only "vector" namespace in metadata can be used to
determine which namespace is being used.



### Types of Data

#### Data

This is the main content of a log. The data that is decoded with the configured decoder will be placed on the root of the event.

There is a special case when the codec is "bytes", since the data is just a string in that case. That means
that the root is a string. Historically this has been forbidden, but it is possible to allow this.
An example of this is the "socket" source, with the "bytes" codec.


#### Metadata

This is any useful data from the source that is not placed on the event. This will be stored in event metadata.

Vector metadata, such as `ingest_timestamp` and `source_type` will be added here nested under the `vector` namespace.

Source metadata will be name-spaced using the name of the source type.

#### Secret Metadata

Secrets such as the `datadog_api_key` and `splunk_hec_token` will be placed in their own container to make it more
difficult to accidentally access / leak. VRL functions will be provided to access secrets, similar to the existing
`get_metadata_field` / `set_metadata_field` today.


### Codecs

#### Bytes / Json

Decoded data will be placed either at the root or nested depending on the source.


#### Syslog

All data will be placed at the root. Since the structure is known, it's easy to prevent naming collisions here.
Syslog has a "message" field which might be useful to apply an additional codec to, but this is currently not supported,
and no support is being added here.

#### Native / Native JSON

When these are used in the "Vector" source, the behavior will not change, and events will be passed through as-is.

### Metadata

There are 3 sources of information that will be stored in Metadata.

1. Vector "internal" metadata such as `datadog_api_key`, so it can be read/set by users. This data already exists, but will
be moved to its own "secret" metadata. VRL functions will be added for accessing "secret" metadata.
2. Vector metadata such as `ingest_timestamp`, and `source_type` which are set for every source. These will be nested under `vector'
3. Source metadata which will vary for each source. This will be nested under the source type.

There is currently minimal support for event metadata. Several changes will need to be made.

Changes needed immediately:

- Support arbitrarily nested values
- Functions that access metadata (`get_metadata_field`, `remove_metadata_field`, and `set_metadata_field`) should support full paths as keys, and return
  the `any` type instead of just `string`.
- Add a separate "secret" metadata.
- New VRL functions to access "secret" metadata.

With these changes, using metadata can still be a bit annoying since the returned type will always be `any`, even if the
value is set and read in the same VRL program. Future enhancements will improve this.

### Future enhancements

- Add a special path syntax to reference metadata (and secrets) instead of using functions.
- Expand schema support to metadata
- Expand semantic meaning to metadata
- Allow sources to be configured to pull in additional data into the event (from the metadata)
- Allow sinks to be configured to pull in additional data from metadata
- Persist metadata in disk buffers. Sinks will require metadata to be able to differentiate between namespaces.

### Implementation

A proof of concept for the Datadog Agent Logs source is being worked on alongside this RFC: [12218](https://github.com/vectordotdev/vector/pull/12218)

### Examples

All examples shown are using the new Vector namespace

#### Datadog Agent source / JSON codec

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
  "datadog_agent": {
    "ddsource": "waters",
    "ddtags": "env:prod",
    "hostname": "beta",
    "service": "cernan",
    "status": "notice",
    "timestamp": "2066-08-09T04:24:42.1234Z" // This is parsed from a unix timestamp provided by the DD agent
  },
  "vector": {
    "source_type": "datadog_agent",
    "ingest_timestamp": "2022-04-14T19:14:21.899623781Z"
  }
}
```

secrets (this will look similar for all sources, so it is emitted on the remaining examples)

```json
{
  "datadog_api_key": "2o86gyhufa2ugyf4",
  "splunk_hec_token": "386ygfhawnfud6rjftg"
}
```

-----

#### Datadog Agent source / bytes codec

event

```json
"{\"proportional\":702036423,\"integral\":15089925750456892008,\"derivative\":-6.4676193438086e263,\"vegetable\":20003,\"mineral\":\"vsd5fwYBv\"}"
```

metadata

```json
{
  "datadog_agent": {
    "message": ,
    "ddsource": "waters",
    "ddtags": "env:prod",
    "hostname": "beta",
    "service": "cernan",
    "status": "notice",
    "timestamp": "2066-08-09T04:24:42.1234Z"
  },
  "vector": {
    "source_type": "datadog_agent",
    "ingest_timestamp": "2022-04-14T19:14:21.899623781Z"
  }
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
  "kafka": {
    "key": "the key of the message"
    // headers were originally nested under a configurable "headers_key". This is using a static value.
    "headers": {
      "header-a-key": "header-a-value",
      "header-b-key": "header-b-value"
    }
    "topic": "name of topic",
    "partition": 3,
    "offset": 1829448,
  },
  "vector": {
    "log_namespace": "vector",
    "source_type": "kafka",
    "ingest_timestamp": "2022-04-14T19:14:21.899623781Z"
  }
}
```

-----

#### Kubernetes Logs / Vector namespace

event (a string as the root event element)

```json
"F1015 11:01:46.499073       1 main.go:39] error getting server version: Get \"https://10.96.0.1:443/version?timeout=32s\": dial tcp 10.96.0.1:443: connect: network is unreachable"
```

metadata

```json
{
  "kubernetes_logs": {
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
    "stream": "stderr"
  },
  "vector": {
    "source_type": "kubernetes_logs",
    "ingest_timestamp": "2020-10-15T11:01:46.499555308Z"
  }
}
```

-----


#### Syslog / Vector namespace

event

```json
"Hello Vector"
```

metadata

```json
{
  "syslog": {
    "source_ip": "127.0.0.1",
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
  },
  "vector": {
    "source_type": "syslog",
    "ingest_timestamp": "2020-10-15T11:01:46.499555308Z"
  }
}
```

-----

#### Socket source (mode=udp) / syslog codec / Vector namespace

event

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
  "socket": {
    "source_ip": "192.168.0.1",
    "hostname": "localhost"
  },
  "vector": {
    "source_type": "socket",
    "ingest_timestamp": "2020-10-15T11:01:46.499555308Z"
  }
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
  "http": {
    "path": "/foo/bar",
    // headers and query params were previously placed directly on the root. This needs to be nested to avoid potential naming conflicts.
    "headers": {
      "Content-Type": "application/json"
    },
    "query_params": {
      "page": 14,
      "size": 3
    }
  },
  "vector": {
    "source_type": "http",
    "ingest_timestamp": "2020-10-15T11:01:46.499555308Z"
  }
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
  "kafka": {
    "key": "the key of the message (from the 2nd kafka source)"
    "headers": {
      "header-a-key": "header-a-value (from the 2nd kafka source)",
      "header-b-key": "header-b-value (from the 2nd kafka source)"
    }
    "topic": "name of topic",
    "partition": 3,
    "offset": 1829448
  },
  "vector": {
    "source_type": "kafka",
    "ingest_timestamp": "2022-04-14T19:14:21.899623781Z"
  }
}
```



## Prior Art

- OpenTelemetry: https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/logs/data-model.md


## Plan Of Attack

- Complete the proof of concept PR for the Data Agent Logs source. ([12218](https://github.com/vectordotdev/vector/pull/12218))
- Add log namespace support to each component

