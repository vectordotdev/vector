# RFC 6330 - 2021-04-07 - Converting one log event into multiple log events

This RFC describes an approach for emitting multiple log events from an `explode` transform.

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

The proposal is to add an `explode` transform that makes use of the Vector Remap Language (VRL) to emit a set of events from one input event by requiring the VRL program to resolve to an array. For each element of the array, a separate event will be published.

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

Ouput:

```json
{"message": "foo"}
{"message": "bar"}
```

Support for iteration as part of [#6031](https://github.com/timberio/vector/issues/6031) will allow for users to do things like map fields onto each element. An example might look something like:

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

This is similar to the support that the current [`explode` transform PR](https://github.com/timberio/vector/pull/6545) has for merging in top-level fields when creating events from a subfield that has an array.


## Doc-level Proposal

To convert an incoming event into multiple events, the `explode` transform can be used. This transform takes a VRL program where it expects the last expression to return an array.

For example, given an input of:

```json
{ "events": [{ "message": "foo" }, { "message": "bar" }] }
```

And a transform of:

```toml
[transforms.explode]
type = "explode"
source = "array!(.events) ?? []"
```

The following events will be output:

```json
{"message": "foo"}
{"message": "bar"}
```

If any elements in the returned array are not an object, they will be turned into a string and set as the `message` key on event.

## Rationale

This enhances Vector with a native transform capable of transforming an event into multiple events. Without this, users will continue to have to use Lua or WASM to acheive this, which introduces a performance bottleneck compared to this proposal.

## Prior Art

- [fluent-plugin-record_splitter](https://github.com/ixixi/fluent-plugin-record_splitter)
- [Logstash split](https://www.elastic.co/guide/en/logstash/current/plugins-filters-split.html)

These are similar to the proposed approach.

## Drawbacks

- Adds an additional transform to be aware of.
- Less flexible than `emit_log` alternative which could emit arbitrary events from `remap` transform.
- This idea of the last line of the remap script being the "return value" could be a bit confusing to users in how it differs from `remap` which uses whatever `.` is at the end of the script. However, this is similar to VRL's use in unit tests and conditions for the `filter` and `route` transforms.
- Ongoing maintenance burden should be minimal.

## Alternatives

### emit_log function

(previous proposal)

This is roughly the same as https://github.com/timberio/vector/issues/6330#issuecomment-772562955 with some slight tweaks.

A new `emit_log` function will be added to the VRL stdlib

```text
emit_log(value: Object)
```

This function will cause the object passed as value to be emitted at that point and flushed downstream. The emitted log will have its metadata copied from the input event.

Additionally, an `emit_root` (we can work on the naming) config option will be added to the `remap` transform to configure whether `.` is emitted after the transform runs. It will default to `true` to preserve the current behavior but can be set to `false` by users to supress this behavior. Admittedly, I'm not wild about introducing this additional config option, but I'm not seeing another great alternative.

This will be able to be combined with the iteration mechanism that will be introduced [#6031](https://github.com/timberio/vector/issues/6031) to emit an unknown number of events. Naively this might look something like:

```text
for stooge in .stooges
   emit_log(stooge)
end
```

In the future we can also add functions for emitting metrics like:

```text
emit_counter(namespace: String, name: String, timestamp: Timestamp, value: Float, kind: "absolute"|"relative")
```

I considered having just an `emit_metric()` but it would require users to pass in objects that match exactly the internal representation we have for metrics.

The `remap` tranform would gain an extra configuration option:

```text
emit_root = true/false # default false
```

When `emit_root` is `true`, the value of `.` will be emitted at the end of the remap program. When `emit_root` is false, the value of `.` will not be emitted. Instead users should use the `emit_log` function to emit.

We could avoid having an `emit_root` config option on the remap transform by just not emitting automatically if we see an `emit_log` function in the user-provided source. I personally think this would be a bit suprising, but it is an option.

### Modifying remap to accept setting the root object to an array

https://github.com/timberio/vector/issues/6988

This would modify remap to allow setting `.` to an array of objects to have each element emitted independently.

This turned out to require a bigger change than I expected in that `.` is linked to mutating the underlying event (metric or log). It's definitely doable, but would require a substantial refactoring and so caused me to take a step back and consider the alternatives, prompting this RFC.

Using a separate `explode` transform keeps the responsibilities of the transform more clear and avoids having to refactor the `remap` transform to decouple `.` from the underlying Vector `Event` object; though we may still want to do this in the future anyway.

## Plan Of Attack

1. Implement `explode` transform

[1]: https://github.com/timberio/vector/issues/6330#issue-799809382
[2]: https://discord.com/channels/742820443487993987/764187584452493323/808744293945704479
