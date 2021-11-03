---
date: "2021-07-16"
title: "Mapping one log event into multiple"
description: "Introducing new methods for converting a single log event into multiple using the `remap` transform"
authors: ["jszwedko"]
pr_numbers: []
release: "0.15.0"
hide_on_release_notes: false
badges:
  type: new feature
  domains: ["vrl", "remap transform"]
---

This release enables transforming a single log event into multiple log events using the `remap` transform. The
groundwork for this was laid in 0.14.0, but we've been waiting to announce it until the addition of the
[`unnest`][unnest] function in this release.

The basic premise is that if you assign an array to the root object, `.`, in a VRL program run by `remap`, then `remap`
will create one log event for each event in the array.

For example:

```toml
[transforms.remap]
type = "remap"
inputs = []
source = """
. = [{"message": "hello"}, {"message": "world"}]
"""
```

Would generate two output events:

```json
{"message": "hello"}
{"message": "world"}
```

Any array elements that are not key/value objects will be converted to an log event by creating a log event with the
`message` key set to the array element value.

See the [`remap` transform][remap_multiple] docs for more examples.

Additionally, to make it easier to convert an incoming log event into an array, we've added an [`unnest`][unnest]
function to VRL that transforms an incoming event where one of the fields is an array into an array of events, each with
one of the elements from the array field. This is easiest to see with an example:

```toml
[transforms.remap]
type = "remap"
inputs = []
source = """
. = {"host": "localhost", "events": [{"message": "hello"}, {"message": "world"}]} # to represent the incoming event

. = unnest(.events)
"""
```

Would output the following log events:

```json
{ "events": { "message": "hello" }, "host": "localhost" }
{ "events": { "message": "world" }, "host": "localhost" }
```

In the future, we plan to add functionality to VRL to allow iterating over arrays, but, for now, the simple case of
mapping each event in an array separately can be done by having one `remap` transform do the "exploding" of the event,
and another `remap` transform to receive each new event.

An example of this:

```toml
[transforms.explode]
type = "remap"
inputs = []
source = """
. = {"host": "localhost", "events": [{"message": "hello"}, {"message": "world"}]} # to represent the incoming event

. = unnest(.events)
"""

[transforms.map]
type = "remap"
inputs = ["explode"]
source = """
# example of pulling up the nested field to merge it into the top-level
. |= .events
del(.events)
"""
```

Would output the following log events:

```json
{ "message": "hello", "host": "localhost" }
{ "message": "world", "host": "localhost" }
```

[unnest]: /docs/reference/vrl/examples/#unnest
[remap_multiple]: /docs/reference/configuration/transforms/remap/#emitting-multiple-log-events
