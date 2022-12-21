---
date: "2022-04-25"
title: "New `native` and `native_json` codecs"
description: ""
authors: ["lukesteensen"]
pr_numbers: []
release: "0.22.0"
hide_on_release_notes: false
badges:
  type: "announcement"
---

We have added new experimental `native` and `native_json` codecs that allow
users to provide Vector with data directly in its native format. This allows
simpler, more efficient configuration of some often-requested use cases.

## Sending metrics to generic sources

Generic event sources like the `http` source or `exec` source can now directly
receive metrics rather than needing to pass them through a `log_to_metric`
transform.

For example, an `exec` source can now be configured to receive events via the
`native_json` codec:

```toml
[sources.in]
type = "exec"
mode = "scheduled"
command = ["./scrape.sh"]
decoding.codec = "native_json"
```

If `scrape.sh` contained:

```bash
#!/usr/bin/env bash

echo $RANDOM | jq --raw-input --compact-output \
'{
  metric: {
    name: "my_metric",
    counter: {
      value: .|tonumber
    },
    kind: "incremental",
  }
}'
```

Vector would read in counters with random values.

Logs are just simple JSON objects, but it is typical to include a `message` and
a `timestamp` key like:

```json
{
  "log": {
    "message": "hello world",
    "timestamp": "2019-10-12T07:20:50.52Z"
  }
}
```

The specific JSON schema here is subject to change, but you can find an initial
schema [here][cue schema]. The protobuf schema for the `native` codec is the
same used in the `vector` source and sink, and the definition can be found
[here][proto schema].

We will be providing more thorough guidance for using each as the feature
matures. One current limitation of the JSON-based native codec is that timestamp
fields within log messages will be converted to strings. This is partially due
to limitations with JSON itself, but we are exploring workarounds.

## Sending events between Vector instances

Another example use case is Vector-to-Vector communication via a transport other
than our existing gRPC-based `vector` source and sink. Using the `native`
encoding on a source/sink pair like `kafka` will utilize the same Protocol
Buffers-based encoding scheme, providing compact binary messages that
deserialize directly to the same native representation within Vector.

Example source configuration:

```toml
[sources.in]
type = "kafka"
bootstrap_servers = "localhost:9092"
topics = ["vector"]
decoding.codec = "native"
```

This would allow an instance of Vector to receive events from another Vector
instance that has a `kafka` sink configured like:

```toml
[sinks.out]
type = "kafka"
inputs = ["..."]
bootstrap_servers = "localhost:9092"
topic = "vector"
encoding.codec = "native"
```

Note that the new `native` encoding option is not yet documented on sinks as we
are waiting until it is fully rolled out to document and announce the feature;
however, is available on `kafka` and some other sinks.

[cue schema]: https://github.com/vectordotdev/vector/blob/master/lib/codecs/tests/data/native_encoding/schema.cue
[proto schema]: https://github.com/vectordotdev/vector/blob/master/lib/vector-core/proto/event.proto
