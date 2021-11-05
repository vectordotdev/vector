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

When either of `drop_on_error` or `drop_on_abort` is set to `true`, events that
are dropped from the primary output stream due to either errors or aborts are
written instead to a separate output called `dropped`.

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

[transforms.remap]
  type = "remap"
  inputs = ["in"]
  drop_on_error = true
  drop_on_abort = true
  source = """
    . |= object!(parse_json!(.message))
    if .foo == "baz" {
        abort
    }
    .processed = true
  """

[sinks.out]
  type = "console"
  inputs = ["remap"]
  encoding.codec = "json"

[sinks.errors_out]
  type = "console"
  inputs = ["remap.dropped"]
  encoding.codec = "json"
```

You would expect to see output like the following:

```json
{"foo":"bar","message":"valid message","processed":true,"timestamp":"2021-11-05T00:42:03.945157398Z"}
{"foo":"bar","message":"valid message","processed":true,"timestamp":"2021-11-05T00:42:04.945155276Z"}
{"message":"{ \"message\": \"valid message\", \"foo\": \"baz\"}","metadata":{"component":"remap","error":"aborted"},"timestamp":"2021-11-05T00:42:05.945588208Z"}
{"foo":"bar","message":"valid message","processed":true,"timestamp":"2021-11-05T00:42:06.944919061Z"}
{"message":"{ \"message\": \"valid message\", \"foo\": \"baz\"}","metadata":{"component":"remap","error":"aborted"},"timestamp":"2021-11-05T00:42:07.944824028Z"}
{"message":"{ \"message\": \"valid message\", \"foo\": \"baz\"}","metadata":{"component":"remap","error":"aborted"},"timestamp":"2021-11-05T00:42:08.945446981Z"}
{"message":"invalid message","metadata":{"component":"remap","error":"function call error for \"object\" at (9:39): function call error for \"parse_json\" at (17:38): unable to parse json: expected value at line 1 column 1"},"timestamp":"2021-11-05T00:42:09.945394161Z"}
{"message":"{ \"message\": \"valid message\", \"foo\": \"baz\"}","metadata":{"component":"remap","error":"aborted"},"timestamp":"2021-11-05T00:42:10.945183635Z"}
{"message":"invalid message","metadata":{"component":"remap","error":"function call error for \"object\" at (9:39): function call error for \"parse_json\" at (17:38): unable to parse json: expected value at line 1 column 1"},"timestamp":"2021-11-05T00:42:11.944980725Z"}
{"message":"invalid message","metadata":{"component":"remap","error":"function call error for \"object\" at (9:39): function call error for \"parse_json\" at (17:38): unable to parse json: expected value at line 1 column 1"},"timestamp":"2021-11-05T00:42:12.944970623Z"}
{"foo":"bar","message":"valid message","processed":true,"timestamp":"2021-11-05T00:42:13.945360616Z"}
```

All of the events that were valid JSON were processed and output as JSON via the
`out` console sink, while those that failed are written out in plain text via the
`errors_out` console sink.
