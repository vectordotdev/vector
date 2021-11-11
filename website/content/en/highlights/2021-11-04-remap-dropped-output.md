---
date: "2021-11-04"
title: "New `dropped` output on the `remap` transform"
description: "Sending failed events down a separate pipeline"
authors: ["lukesteensen"]
pr_numbers: [9169]
release: "0.18.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["remap"]
---

Vector 0.18 introduces a new `dropped` output to the `remap` transform. This
can be used to route events that fail processing down a separate pipeline.

When either of `drop_on_error` or `drop_on_abort` is set to `true` and the new
`reroute_dropped` config is also set to `true`, events that are dropped from the
primary output stream due to either errors or aborts are written instead to
a separate output called `dropped`. Those events are written out to the
`dropped` output in their original form, so no modifications that occurred
before the error or abort will be visible.

## Example

As an example, the `dropped` output can be used if you want to capture events
that failed during processing and send them out via a separate sink from the
data the was processed successfully.

Given a config of:

```toml
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

You would expect to see output like the following (formatted for clarity):

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

All of the events that were valid JSON were processed and output via the `foo`
console sink, while those that either errored or were aborted are written out in
via the `bar` console sink with the relevant metadata added.
