# RFC 6330 - 2021-04-07 - Emitting multiple log events from remap transform

This RFC describes an apporach for emitting multiple log events from a remap transform.

## Scope

In:
- Emitting multiple log events from a remap transform

Out:
- Emitting multiple metrics from a remap transform

## Motivation

Currently the remap transform can only modify an event as it is passing through. Users have asked [1][] [2][] to be able to transform an incoming event into multiple events. This is useful, for example, when parsing an incoming JSON payload that contains an array for which you'd like to publish an event for each element.

Currently, users are restricted to using the Lua or WASM transform for this, [introducing a substantial bottleneck](https://user-images.githubusercontent.com/316880/105531520-d3643200-5cbf-11eb-8b35-fe1c99e5c254.png).

Additionally, I think we can leverage the approach I describe here in the future to allow users to emit different data types than the incoming type. For example, users could emit logs related to an incoming metric, or metrics from an incoming log.

## Internal Proposal

This is roughly the same as https://github.com/timberio/vector/issues/6330#issuecomment-772562955 with some slight tweaks.

A new `emit_log` function will be added to the VRL stdlib:

```
emit_log(value: Object)
```

This function will cause the object passed as value to be emitted at that point and flushed downstream. The emitted log will have its metadata copied from the input event.

Additionally, an `emit_root` (we can work on the naming) config option will be added to the `remap` transform to configure whether `.` is emitted after the transform runs. It will default to `true` to preserve the current behavior but can be set to `false` by users to supress this behavior. Admittedly, I'm not wild about introducing this additional config option, but I'm not seeing another great alternative.

In the future we can also add functions for emitting metrics like:

```
emit_counter(namespace: String, name: String, timestamp: Timestamp, value: Float, kind: "absolute"|"relative")
```

I considered having just an `emit_metric()` but it would require users to pass in objects that match exactly the internal representation we have for metrics.

## Doc-level Proposal

The `remap` tranform would gain an extra configuration option:

```
emit_root = true/false # default false
```

When `emit_root` is `true`, the value of `.` will be emitted at the end of the remap program. When `emit_root` is false, the value of `.` will not be emitted. Instead users should use the `emit_log` function to emit.

## Rationale

This enhances remap to support additional transformation use-cases of splitting up an event into multiple events. Without this, users will continue to have to use Lua to acheive this, which introduces a performance bottleneck compared to remap

## Prior Art

- [fluent-plugin-record_splitter](https://github.com/ixixi/fluent-plugin-record_splitter)
- [Logstash split](https://www.elastic.co/guide/en/logstash/current/plugins-filters-split.html)

These are similar to the `explode` transform PR we have: https://github.com/timberio/vector/pull/6545

## Drawbacks

- It makes the remap transform more complicated to understand and adds an additional configuration option that users need to be aware of
- Ongoing maintenance burden should be minimal

## Alternatives

### Not emitting . if an `emit_log` is present in the program

We could avoid having an `emit_root` config option on the remap transform by just not emitting automatically if we see an `emit_log` function in the user-provided source. I personally think this would be a bit suprising, but it is an option.

### Modifying remap to accept setting the root object to an array

https://github.com/timberio/vector/issues/6988

This would modify remap to allow setting `.` to an array of objects to have each element emitted independently.

This turned out to require a bigger change than I expected in that `.` is linked to mutating the underlying event (metric or log). It's definitely doable, but would require a substantial refactoring and so caused me to take a step back and consider the alternatives, prompting this RFC.

### An `explode` transformation

https://github.com/timberio/vector/pull/6545

Have a separate, special purpose, transform for converting a single log event into multiple log events.

This would work, but I think the downsides are:

* Introduces a new transform rather than just leveraging an existing one, remap. There is some precedence for this with the `route` and `reduce` transforms though.
* It is less flexible than letting users emit arbitrary events
* There isn't a clear path to emitting, say, multiple metrics from an input log event

## Plan Of Attack

TODO, but [something like what Jean wrote](https://github.com/timberio/vector/issues/6330#issuecomment-772562955).

[1]: https://github.com/timberio/vector/issues/6330#issue-799809382
[2]: https://discord.com/channels/742820443487993987/764187584452493323/808744293945704479
