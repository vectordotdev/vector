---
title: Using Vector `tap`
short: Vector tap
description: Learn how to use the Vector `tap` CLI command to examine events as they flow through your pipeline and troubleshoot issues.
author_github: https://github.com/001wwang
domain: operations
weight: 6
tags: ["cli", "tap", "level up", "guides", "guide"]
---

{{< requirement >}}
Before you begin, this guide assumes the following:

* You understand the [basic Vector concepts][concepts]
* You understand [how to set up a basic pipeline][pipeline]

[concepts]: /docs/about/concepts
[pipeline]: /docs/setup/quickstart
{{< /requirement >}}

Vector's `tap` CLI command allows you to observe events as they flow to and from
components in your pipelines. If you've ever attached a `console` sink to an
output for debug purposes and wondered if there's a better way, you're in the
right place. This guide walks you through how `vector tap` can be used to level
up your troubleshooting experience.

## Getting Started

To start, we'll reference the following base configuration, but feel free to
substitute your own. Just note that the [Vector API] must be enabled for `vector
tap` to work. See [under the hood](#under-the-hood) for more details.

```toml
[api]
enabled = true

[sources.in]
type = "demo_logs"
format = "shuffle"
lines = [
  "test1",
  "test2",
]

[sinks.out]
type = "blackhole"
inputs = ["in*"]
```

Run Vector with this configuration and watch the configuration for changes.

```console
vector --config path/to/config.toml -w
```

Now run `vector tap`! You should start seeing a stream of
notifications (sent to `stderr`) and events (sent to `stdout`) in your terminal.

```console
[tap] Pattern "*" successfully matched.
{"message":"test1","source_type":"demo_logs","timestamp":"2022-02-22T19:20:40.487671258Z"}
{"message":"test2","source_type":"demo_logs","timestamp":"2022-02-22T19:20:41.486858019Z"}
...
```

Notifications are informative messages prefixed with `[tap]` and, in this case,
events are logs flowing out of the `demo_logs` component `in`. There's a lot
going on here -- component patterns, events from outputs, JSON formatting -- so
let's start by unpacking it all.

## Usage Basics

### Component Patterns

You can `tap` both the output events of components (sources or transforms) and
the input events of components (transforms or sinks) by providing component ID
patterns in the `--outputs-of` and `--inputs-of` options respectively. More
specifically, glob patterns are accepted in these options. You can also specify
output patterns as additional arguments.

Running the bare `vector tap` command invokes sensible defaults and is
equivalent to specifying `--outputs-of "*"` where `"*"` matches any component
ID.

Try running `vector tap --inputs-of "out"`. The events should be the same
as before, but you're now tapping the input events of the `blackhole` sink
`out`.

{{< info >}}
Note that tapping a component's input is effectively a shorthand for
tapping all the outputs that feed into that component.
{{< /info >}}

### Customizing output

`tap` notifications provide additional context such as information about pattern
matching success or failure and improper usage of patterns. If you'd like to
hide notifications, use the `--quiet` option.

By default, `tap` outputs events encoded in JSON format. YAML and logfmt are
also supported and can be enabled by using the `--format` option.

Events in `tap` are actually sampled from their tapped components for
performance and reliability. You can change the time interval of each sample
with `--interval` and the maximum number of events to sample per interval with
`--limit`.

Try running `vector tap --quiet --format logfmt`. You'll now see no
notifications and events encoded as logfmt.

```console
...
message=test1 source_type=demo_logs timestamp=2022-02-22T20:57:01.430905309Z
message=test1 source_type=demo_logs timestamp=2022-02-22T20:57:02.430987800Z
...
```

### Configuration reloading support

`tap` is compatible with configuration reloading. In other words, if you add,
remove, or edit existing components in your configuration, `tap` will adapt
accordingly by re-matching your provided patterns.

With `vector tap` running, add the following `demo_logs` source to the base
configuration.

```toml
[sources.in-2]
type = "demo_logs"
format = "shuffle"
lines = [
  "new test1",
  "new test2",
]
```

You'll now see events from component `in-2` appear in `tap` output.

```console
...
{"message":"new test2","source_type":"demo_logs","timestamp":"2022-02-22T21:07:50.106793803Z"}
{"message":"test1","source_type":"demo_logs","timestamp":"2022-02-22T21:07:50.490873070Z"}
{"message":"new test1","source_type":"demo_logs","timestamp":"2022-02-22T21:07:51.106744949Z"}
...
```

You can read more about all available options in the [Vector tap docs] or by
running `vector tap --help`.

## Troubleshooting Example

With the basic mechanics out of the way, let's see how you might use `tap` to
troubleshoot a pipeline.

We'll use the following Vector configuration.

```toml
[api]
enabled = true

[sources.in]
type = "demo_logs"
format = "shuffle"
lines = [
  '{ "type": "icecream", "flavor": "strawberry" }',
  '{ "type": "icecream", "flavor": "chocolate" }',
  '{ "type": "icecream", "flavor": "wasabi" }',
]

[transforms.picky]
type = "remap"
inputs = ["in"]
drop_on_abort = true
reroute_dropped = true
source = '''
  if .flavor == "strawberry" {
    .happiness = 10
  } else if .flavor == "chocolate" {
    .happiness = 5
  } else {
    abort
  }
'''

[sinks.store]
type = "console"
inputs = ["picky"]
target = "stdout"
encoding.codec = "json"

[sinks.trash]
type = "blackhole"
inputs = ["picky.dropped"]
```

Running this configuration, we expect to see our favorite ice cream logs appear
in `stdout`. Unfortunately, we see nothing at all. The desired events don't look
like they're ever reaching their destination.

We can verify this by examining the inputs of the `store` sink with `vector tap
--inputs-of "store"`: indeed, no events appear. We can also narrow in and
inspect the output of relevant upstream components like our `remap` transform.
`vector tap --outputs-of "picky"` (which, in this case, is effectively the same
as inspecting the inputs of `store`) shows that events are not flowing.

Are all the events being dropped instead? A quick glance with `vector tap
--outputs-of "picky.dropped"` confirms that suspicion as `tap` starts displaying
a stream of all our dropped logs. On closer examination, it's clear that our
events don't have the shape we expected.

```jsonc
{"message":"{ \"type\": \"icecream\", \"flavor\": \"strawberry\" } }
```

There's no `.flavor` field for our conditional to run on. Instead, the entire
payload from our source has been included in the default `message` field. Right,
we forgot to parse the payload into JSON. We need to add the following line in
our VRL source code:

```coffeescript
. = parse_json!(.message)
```

With that modification and a quick reload of the configuration, we start seeing
our precious ice cream logs in the console. Hurray!

While this was a highly contrived example, the issues highlighted are relevant
to many real world troubleshooting scenarios: unexpected input from sources,
misconfigured transformations, missing clarity on where events end up and how
they're structured. We hope `vector tap` eases the troubleshooting burden
especially in larger, more complex setups where mistakes are not easy to find
by simply reading a configuration file.

Ultimately, `vector tap` is one tool in the wide range of Vector features that
lets you safely and reliably set up your observability pipelines. Be sure to
also check out [Vector unit testing] to verify the expected behavior of your
transforms and [Vector internal observability] for more insight into Vector
itself.

## Under the hood

Under the hood, `vector tap` is powered by the [Vector API], specifically by the
`outputEventsByComponentIdPatterns` subscription. If you'd like a more direct
and programmatic way to examine events in your pipeline, consider interacting
with the API directly.

{{< info >}}
Note that as long as your Vector instance has its API enabled and exposed,
`vector tap` will work! Simply use the `--url` option to specify a non-default
API address. This allows you to troubleshoot remote Vector instances.
{{< /info >}}

We encourage contributions and suggestions for improving `vector tap`!

[Vector API]: /docs/reference/api
[Vector tap docs]: /docs/reference/cli/#tap
[Vector unit testing]: /docs/reference/configuration/unit-tests
[Vector internal observability]: /docs/administration/monitoring
