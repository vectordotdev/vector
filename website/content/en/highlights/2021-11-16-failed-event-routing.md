---
date: "2021-11-16"
title: "Failed event routing feature released"
description: "Routing failed events from the `remap` transform"
authors: ["barieom", "lukesteensen"]
pr_numbers: []
release: "0.18.0"
hide_on_release_notes: false
badges:
  type: new feature
---

# Failed event routing

We've released a new feature that enables users to route failed events through separate pipelines.

Previously, when Vector encountered an event that failed a given transformation, the event was either dropped or was forwarded to the next step of the process. 

With this new release, Vector enables events that fail to process down a different pipeline without any manual work-around to catch errors, tag, fanout, or filter. In other words, users can now handle failed events in a way the user sees fit, such as routing the failed events to another sink for storage, inspection, and replay. 

As an example, the `dropped` output can be used if you want to capture events that failed during processing and re-direct the failed events to a separate sink from the data that was processed successfully. In a given config below:

``` toml
[sources.in]
  type = "generator"
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
  inputs = ["my_remap.dropped"]
  encoding.codec = "json"
``` 

This would emit the following output:
``` 
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

Events that either caused an error or were aborted are written out by the `bar` console sink with the relevant metadata added; on the other hand, events that were valid JSON were processed and output by the `foo` console sink. 

For our next steps, we'll be looking to add `filtering` functionality, which will enable events to be passed to the next processor if a condition is not. In the meantime, if you any feedback for us, let us know on our [Discord chat][] or on [Twitter][]!


[Discord chat]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev
