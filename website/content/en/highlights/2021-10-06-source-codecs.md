---
date: "2021-10-06"
title: "New `decoding` and `framing` options for sources"
description: ""
authors: ["jszwedko"]
pr_numbers: []
release: "0.17.0"
hide_on_release_notes: false
badges:
  type: "announcement"
---

Often, when consuming data from a source, the first operation you have to do on
it is decode the data from its source representation. To make this easier, we've
added new `decoding` options to [most sources][9404].

For example, if you have a `kafka` source that has JSON-encoded messages, now
you can simply add `decoding.codec = "json"` to your source configuration like:

```toml
[sources.kafka]
type = "kafka"
bootstrap_servers = "localhost:9200"
topics = ["my_topic"]
decoding.codec = "json"
```

This will decode your messages from JSON, thus saving you from an additional
`remap` transform.

In addition, we've added a new `framing` option to allow configuration for
sources that have non-standard framing (for example a custom-delimiter
separating messages).

For example, if you have an `http` source where the messages are delimited by
commas instead of newlines, you can configure this like:

```toml
[sources.http]
type = "http"
address = "0.0.0.0:8080"
framing.method = "character_delimited"
framing.character_delimited.delimiter = ","
```

To have Vector parse each comma-delimited element as a new message. This can be
used with `decoding` option specified above. See the [docs][http_source_framing]
for other framing options.

[http_source_framing]: /docs/reference/configuration/sources/http_server/#framing
[9404]: https://github.com/vectordotdev/vector/issues/9404
