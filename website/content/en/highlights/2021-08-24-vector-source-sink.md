---
date: "2021-08-20"
title: "Version 2 of the Vector source/sink released"
description: ""
authors: ["JeanMertz"]
pr_numbers: []
release: "0.16.0"
hide_on_release_notes: false
badges:
  type: "announcement"
---

We've released a new major version (`v2`) of our `vector` [source][]/[sink][]
components. This release resolves several issues and limitations we experienced
with our previous (`v1`) TCP-based implementation of these two components:

- `vector` sink does not work in k8s with dynamic IP addresses ([#2070][])
- Allow for HTTP in the vector source and sinks ([#5124][])
- Allow Vector Source and Sink to Communicate over GRPC ([#6646][])
- RFC 5843 - Encoding/Decoding for Vector to Vector Communication ([#6032][])

The new version transitions to using gRPC over HTTP as its communication
protocol, which resolves those limitations.

To allow operators to transition at their leisure, this new release of Vector
still defaults to `v1`. In the next release (`0.17.0`) we'll require operators
to explicitly state which version they want to use, but continue to support
`v1`. The release after that (`0.18.0`) we'll drop `v1` completely, and default
to `v2`, we also no longer require you to explicitly set the version since there
will only be one supported going forward.

If you want to opt in to the new (stable!) `v2` version, you can do so as
follows:

```diff
[sinks.vector]
  type = "vector"
+ version = "2"

[sources.vector]
  type = "vector"
+ version = "2"
```

There are a couple of things to be aware of:

#### Removed options

There are some configuration options that are no longer valid with version 2 of
the source and sink:

- `vector` source: `keepalive` and `receive_buffer_bytes`
- `vector` sink: `keepalive` and `send_buffer_bytes`

As these were specific to the TCP-based version 1 of the protocol.

#### Upgrade both the source _and_ sink

You **have** to upgrade **both** the source and sink to `v2`, or none at all,
you cannot update one without updating the other. Doing so will result in a loss
of events.

#### Zero-downtime deployment

If you want to do a zero-downtime upgrade to `v2`, you'll have to introduce the
new source/sink versions next to the existing versions, before removing the
existing one.

First, deploy the configuration that defines the source:

```diff
  [sources.vector]
    address = "0.0.0.0:9000"
    type = "vector"
+   version = "1"

+ [sources.vector]
+   address = "0.0.0.0:5000"
+   type = "vector"
+   version = "2"
```

Then, deploy the sink configuration, switching it over to the new version:

```diff
  [sinks.vector]
-   address = "127.0.1.2:9000"
+   address = "127.0.1.2:5000"
    type = "vector"
+   version = "2"
```

Once the sink is deployed, you can do another deploy of the source, removing the
old version:

```diff
- [sources.vector]
-   address = "0.0.0.0:9000"
-   type = "vector"
-   version = "1"
-
  [sources.vector]
    address = "0.0.0.0:5000"
    type = "vector"
    version = "2"
```

That's it! You are now using the new transport protocol for Vector-to-Vector
communication.

[source]: https://vector.dev/docs/reference/configuration/sources/vector/
[sink]: https://vector.dev/docs/reference/configuration/sinks/vector/
[#2070]: https://github.com/vectordotdev/vector/issues/2070
[#5124]: https://github.com/vectordotdev/vector/issues/5124
[#6646]: https://github.com/vectordotdev/vector/issues/6646
[#6032]: https://github.com/vectordotdev/vector/pull/6032
