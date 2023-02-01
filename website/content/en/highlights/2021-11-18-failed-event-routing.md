---
date: "2021-11-18"
title: "Initial support for routing failed events released"
description: "Routing failed events from the `remap` transform"
authors: ["barieom", "lukesteensen"]
pr_numbers: [9417, 9169]
release: "0.18.0"
hide_on_release_notes: false
badges:
  type: new feature
---

We've released a new feature that enables users to route failed events through
separate pipelines.

Previously, when Vector encountered an event that failed in a given transform
component, the event was either dropped or was forwarded to the next step of the
process.

With this new release, you can configure Vector to send events that fail to
process down a different pipeline, without any manual workaround, to catch
errors, tag, fanout, or filter. In other words, users can now handle failed
events in a way the user sees fit, such as routing the failed events to another
sink for storage, inspection, and replay. We are piloting this feature with the
`remap` transform but plan to roll this out to other transforms in the future so
keep your eyes peeled for announcements down the line. Note: unit tests do not
currently support assertions on failed events but this is also in the works.

For the `remap` transform, there is now a new `.dropped` output that can be used
to catch and route events that would have otherwise been dropped. To use this,
you need to configure `drop_on_error` to `true` and `reroute_dropped` to `true`.
The latter lets you opt into this new feature. Once enabled, you can use the
component id of the `remap` transform suffixed with `.dropped` as an input to
another component to handle failed events differently.

See the below example for how this works.

Let's start with this configuration:

``` toml
[sources.in]
  type = "demo_logs"
  format = "shuffle"
  interval = 1.0
  lines = [
    '{ "message": "valid message", "foo": "bar"}',
    '{ "message": "valid message", "foo": "baz"}',
    'invalid message',
  ]

[transforms.my_remap]
  type = "remap"
  inputs = ["in"]
  drop_on_error = true
  drop_on_abort = true
  reroute_dropped = true
  source = """
    . |= object!(parse_json!(.message))
    if .foo == "baz" {
        abort
    }
    .processed = true
  """

[sinks.foo]
  type = "console"
  inputs = ["my_remap"]
  encoding.codec = "json"

[sinks.bar]
  type = "console"
  inputs = ["my_remap.dropped"] # note the new `.dropped` here!
  encoding.codec = "json"
```

If run, this would emit the following output:

```json
{
  "foo": "bar",
  "message": "valid message",
  "processed": true,
  "timestamp": "2021-11-09T16:11:47.330713806Z"
}
{
  "foo": "bar",
  "message": "valid message",
  "processed": true,
  "timestamp": "2021-11-09T16:11:48.330756592Z"
}
{
  "message": "invalid message",
  "metadata": {
    "dropped": {
      "component_id": "my_remap",
      "component_type": "remap",
      "component_kind": "transform",
      "message": "function call error for \"object\" at (9:39): function call error for \"parse_json\" at (17:38): unable to parse json: expected value at line 1 column 1",
      "reason": "error"
    }
  },
  "timestamp": "2021-11-09T16:11:49.330157298Z"
}
{
  "message": "{ \"message\": \"valid message\", \"foo\": \"baz\"}",
  "metadata": {
    "dropped": {
      "component_id": "my_remap",
      "component_type": "remap",
      "component_kind": "transform",
      "message": "aborted",
      "reason": "abort"
    }
  },
  "timestamp": "2021-11-09T16:11:50.329966720Z"
}
```

Events that either caused an error or were aborted are written out by the `bar`
console sink with the relevant metadata added; on the other hand, events that
were valid JSON were processed and output by the `foo` console sink. More
information is available on the [remap docs page]. As a side note, the
`metadata` key (above under the `"invalid message"`) is configurable via
[`log_schema.metadata_key`][log_schema.metadata_key].

We will be continuing to expand support for routing failed events from other
transforms like `filter`. In the meantime, if you any feedback for us, let us
know on our [Discord chat] or on [Twitter]!

[remap docs page]: /docs/reference/configuration/transforms/remap/
[log_schema.metadata_key]: /docs/reference/configuration/global-options/#log_schema.metadata_key
[Discord chat]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev
