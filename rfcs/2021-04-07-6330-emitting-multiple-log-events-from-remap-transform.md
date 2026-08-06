# RFC 6330 - 2021-04-07 - Converting one log event into multiple log events

This RFC describes an approach for transforming one input log event into multiple log events.

## Scope

In:

- Turning a log event into multiple log events

Out:

- Turning a log event into multiple metrics
- Turning a metric into multiple metrics
- Turning a metric into multiple logs

## Motivation

Users have asked [1][] [2][] to be able to transform an incoming event into multiple events. This is useful, for example, when parsing an incoming JSON payload that contains an array for which you'd like to publish an event for each element.

Currently, users are restricted to using the Lua or WASM transform for this, [introducing a substantial bottleneck](https://user-images.githubusercontent.com/316880/105531520-d3643200-5cbf-11eb-8b35-fe1c99e5c254.png).

## Internal Proposal

The proposal is to:

1. Extend `remap` so that if `.` is an array at the end, it will emit one event for each element in that array
2. Add an `unnest` VRL function that will transform an object into an array of objects using a specified field on the input object

Example input:

```json
{ "host": "localhost", "events": [{ "message": "foo" }, { "message": "bar" }] }
```

Remap transform config:

```toml
[transforms.remap]
type = "remap"
source = """
. = unnest(., "events")
"""
```

Output:

```json
{ "host": "localhost", "message": "foo" }
{ "host": "localhost", "message": "bar" }
```

Additionally, we will provide `only_fields` and `except_fields` as options on the `unnest` function to allow users to
select which fields will be kept. These match similar semantics to the `encoding` options on sinks.

Example input:

```json
{ "timestamp": "2020-12-09T16:09:53+00:00", "host": "localhost", "events": [{ "message": "foo" }, { "message": "bar" }] }
```

Remap transform config:

```toml
[transforms.remap]
type = "remap"
source = """
. = unnest(., "events", only_fields: ["host"])
"""
```

Output:

```json
{ "host": "localhost", "message": "foo" }
{ "host": "localhost", "message": "bar" }
```

Here the `timestamp` field is not preserved.

## Doc-level Proposal

The `remap` transform can also be used to emit multiple events from a single incoming event by setting the root path,
`.`, to an array.

For example, given an input of:

```json
{ "host": "localhost", "events": [{ "message": "foo" }, { "message": "bar" }, 1] }
```

And a transform of:

```toml
[transforms.remap]
type = "remap"
source = """
. = unnest(., "events")
"""
```

The following events will be output:

```json
{ "host": "localhost", "message": "foo" }
{ "host": "localhost", "message": "bar" }
{ "host": "localhost", "message": "1" }
```

That is, each record in the indicated field will be emitted as its own event, merged with any other fields existing at
the top-level of the event.

If any elements in the array field are not an object, they will be set as the `message` key.

## Rationale

This enhances `remap` to be able to emit multiple events. Without this, users will continue to have to use Lua or WASM
to achieve this, which introduces a performance bottleneck compared to this proposal.

## Prior Art

- [fluent-plugin-record_splitter](https://github.com/ixixi/fluent-plugin-record_splitter)
- [Logstash split](https://www.elastic.co/guide/en/logstash/current/plugins-filters-split.html)

These are similar to the proposed approach.

## Drawbacks

- Adds an additional transform to be aware of.
- Less flexible than `emit_log` alternative which could emit arbitrary events from `remap` transform.
- Ongoing maintenance burden should be minimal.

## Alternatives

### explode transform

(previous proposal)

We add an `explode` transform that makes use of the Vector Remap Language (VRL) to emit a set of events from one input
event by requiring the VRL program to resolve to an array. For each element of the array, a separate event will be
published.

Example input:

```json
{ "events": [{ "message": "foo" }, { "message": "bar" }] }
```

Transform config:

```toml
[transforms.explode]
type = "explode"
source = "array!(.events) ?? []" # will be typechecked at compile-time
```

Output:

```json
{"message": "foo"}
{"message": "bar"}
```

Support for iteration as part of [#6031](https://github.com/vectordotdev/vector/issues/6031) will allow for users to do
things like map fields onto each element. An example might look something like:

Input:

```json
{ "host": "foobar", "events": [{ "message": "foo" }, { "message": "bar" }] }
```

Transform config (actual mapping syntax TBA):

```toml
[transforms.explode]
type = "explode"
source = "map(array!(.events), |event| event.host = .host) ?? []"
```

Output:

```json
{"host": "foobar", "message": "foo"}
{"host": "foobar", "message": "bar"}
```

This is similar to the support that the current [`explode` transform PR](https://github.com/vectordotdev/vector/pull/6545)
has for merging in top-level fields when creating events from a subfield that has an array.

### emit_log function

(previous proposal)

This is roughly the same as https://github.com/vectordotdev/vector/issues/6330#issuecomment-772562955 with some slight tweaks.

A new `emit_log` function will be added to the VRL stdlib

```text
emit_log(value: Object)
```

This function will cause the object passed as value to be emitted at that point and flushed downstream. The emitted log
will have its metadata copied from the input event.

Additionally, an `emit_root` (we can work on the naming) config option will be added to the `remap` transform to
configure whether `.` is emitted after the transform runs. It will default to `true` to preserve the current behavior
but can be set to `false` by users to suppress this behavior. Admittedly, I'm not wild about introducing this additional
config option, but I'm not seeing another great alternative.

This will be able to be combined with the iteration mechanism that will be introduced
[#6031](https://github.com/vectordotdev/vector/issues/6031) to emit an unknown number of events. Naively this might look
something like:

```text
for stooge in .stooges
   emit_log(stooge)
end
```

In the future we can also add functions for emitting metrics like:

```text
emit_counter(namespace: String, name: String, timestamp: Timestamp, value: Float, kind: "absolute"|"relative")
```

I considered having just an `emit_metric()` but it would require users to pass in objects that match exactly the
internal representation we have for metrics.

The `remap` transform would gain an extra configuration option:

```text
emit_root = true/false # default false
```

When `emit_root` is `true`, the value of `.` will be emitted at the end of the remap program. When `emit_root` is false,
the value of `.` will not be emitted. Instead users should use the `emit_log` function to emit.

We could avoid having an `emit_root` config option on the remap transform by just not emitting automatically if we see
an `emit_log` function in the user-provided source. I personally think this would be a bit surprising, but it is an
option.

### Modifying remap to accept setting the root object to an array

https://github.com/vectordotdev/vector/issues/6988

This would modify remap to allow setting `.` to an array of objects to have each element emitted independently.

This turned out to require a bigger change than I expected in that `.` is linked to mutating the underlying event
(metric or log). It's definitely doable, but would require a substantial refactoring and so caused me to take a step
back and consider the alternatives, prompting this RFC.

Using a separate `explode` transform keeps the responsibilities of the transform more clear and avoids having to
refactor the `remap` transform to decouple `.` from the underlying Vector `Event` object; though we may still want to do
this in the future anyway.

## Plan Of Attack

1. Modify `remap` to treat setting `.` to an array to indicate that multiple events should be emitted
2. Implement `unnest` VRL function

[1]: https://github.com/vectordotdev/vector/issues/6330#issue-799809382
[2]: https://discord.com/channels/742820443487993987/764187584452493323/808744293945704479
