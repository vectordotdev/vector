---
date: "2021-10-08"
title: "New `errors` output on the `remap` transform"
description: "Sending failed events down a separate pipeline"
authors: ["lukesteensen"]
pr_numbers: [9169]
release: "0.18.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["remap"]
---

Vector 0.18 introduces a new `errors` output to the `remap` transform. This
can be used to route events that fail processing down a separate pipeline.

In order to maintain backwards-compatibility, this behavior must be enabled via
the `drop_on_error` config value. When that is set to `true`, events that
cause an error in VRL will be dropped from the primary output stream and written
instead to a separate output called `errors`.

## Example

As an example, the `errors` output can be used if you want to capture events
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

[transforms.remap]
  type = "remap"
  inputs = ["in"]
  drop_on_error = true
  source = """
    . |= object!(parse_json!(.message))
    .processed = true
  """

[sinks.out]
  type = "console"
  inputs = ["remap"]
  encoding.codec = "json"

[sinks.errors_out]
  type = "console"
  inputs = ["remap.errors"]
  encoding.codec = "text"
```

You would expect to see output like the following:

```json
{"foo":"baz","message":"valid message","processed":true,"timestamp":"2021-10-08T18:04:09.269236640Z"}
{"foo":"baz","message":"valid message","processed":true,"timestamp":"2021-10-08T18:04:10.269192981Z"}
invalid message
invalid message
{"foo":"bar","message":"valid message","processed":true,"timestamp":"2021-10-08T18:04:14.270154463Z"}
{"foo":"baz","message":"valid message","processed":true,"timestamp":"2021-10-08T18:04:15.269096128Z"}
invalid message
{"foo":"baz","message":"valid message","processed":true,"timestamp":"2021-10-08T18:04:17.269055424Z"}
```

All of the events that were valid JSON were processed and output as JSON via the
`out` console sink, while those that failed are written out in plain text via the
`errors_out` console sink.
