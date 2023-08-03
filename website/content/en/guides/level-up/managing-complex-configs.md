---
title: Building and Managing Complex Configs
description: Strategies for building and managing complex Vector configs
author_github: https://github.com/Jeffail
domain: config
weight: 3
tags: ["configuration", "config", "level up", "guides", "guide"]
aliases: ["/docs/setup/guides/advanced-configs"]
---

{{< requirement >}}
Before you begin, this guide assumes the following:

* You understand the [basic Vector concepts][docs.about.concepts]
* You understand [how to set up a basic pipeline][docs.setup.quickstart].

[docs.about.concepts]: /docs/about/concepts
[docs.setup.quickstart]: /docs/setup/quickstart
{{< /requirement >}}

Writing large configuration files is not yet an official olympic event. However,
it's still a good idea to get yourself ahead of the competition. In this guide
we're going to cover some tips and tricks that will help you write clear, bug
free Vector configs that are easy to maintain.

## Generating Configs

In Vector each component of a pipeline specifies which components it consumes
events from. This makes it very easy to build multiplexed topologies. However,
writing a chain of transforms this way can sometimes be frustrating as the
number of transforms increases.

Luckily, the Vector team are desperate for your approval and have worked hard to
mitigate this with the `generate` subcommand, which can be used to generate the
boilerplate for you. The command expects a list of components, where it then
creates a config with all of those components connected in a linear chain.

For example, if we wished to create a chain of three transforms; `remap`, `filter`,
and `reduce`, we can run:

```bash
vector generate /remap,filter,reduce > vector.toml
# Find out more with `vector generate --help`
```

And most of the boilerplate will be written for us, with each component printed
with an `inputs` field that specifies the component before it:

```toml title="vector.toml"
[transforms.transform0]
  inputs = [ "somewhere" ]
  type = "remap"
  # etc ...

[transforms.transform1]
  inputs = [ "transform0" ]
  type = "filter"
  # etc ...

[transforms.transform2]
  inputs = [ "transform1" ]
  type = "reduce"
  # etc ...
```

The IDs of the generated components are sequential (`transform0`,
`transform1`, and so on). It's therefore worth doing a search and replace with
your editor to give them better IDs, e.g. `s/transform2/scrub_emails/g`.

## Testing Configs

Test driven Configuration is a paradigm we just made up, so there's still time
for you to adopt it _before_ it's cool. Vector supports complementing your
configs with [unit tests][guides.unit-testing], and as it turns out
they're also pretty useful during the building stage.

Let's imagine we are in the process of building the config from the [unit test
guide][guides.unit-testing], we might start off with our source and
the grok parser:

```toml title="vector.toml"
[sources.over_tcp]
  type = "socket"
  mode = "tcp"
  address = "0.0.0.0:9000"

[transforms.foo]
  inputs = ["over_tcp"]
  type = "remap"
  source = '''
  . = parse_grok!(.message, s'%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}')
'''
```

A common way to test this transform might be to temporarily change the source
into a `stdin` type, add a `console` sink pointed to our target transform, and
run it with some sample data. However, this is awkward as it means distorting
our config to run tests rather than focusing on features.

Instead, we can leave our source as a `socket` type and add a unit test to the
end of our config:

```toml title="vector.toml"
[[tests]]
  name = "check_simple_log"

  [[tests.inputs]]
    insert_at = "foo"
    type = "raw"
    value = "2019-11-28T12:00:00+00:00 info Sorry, I'm busy this week Cecil"

  [[tests.outputs]]
    extract_from = "foo"
```

When we add a unit test output without any conditions it will simply print the
input and output events of a transform, allowing us to inspect its behavior:

```sh
$ vector test ./vector.toml
Running vector.toml tests
test vector.toml: check_simple_log ... passed

inspections:

--- vector.toml ---

test 'check_simple_log':

check transform 'foo' payloads (events encoded as JSON):
  input: {"timestamp":"2020-02-11T15:04:02.361999Z", "message":"2019-11-28T12:00:00+00:00 info Sorry, I'm busy this week Cecil"}
  output: {"level":"info","message":"Sorry, I'm busy this week Cecil", "timestamp":"2019-11-28T12:00:00+00:00"}
```

As we introduce new transforms to our config we can change the test output
to check the latest transform. Or, occasionally, we can add conditions to an
output in order to turn it into a regression test:

```toml title="vector.toml"
[[tests]]
  name = "check_simple_log"

  [[tests.inputs]]
    insert_at = "foo"
    type = "raw"
    value = "2019-11-28T12:00:00+00:00 info Sorry, I'm busy this week Cecil"

  # This is now a regression test
  [[tests.outputs]]
    extract_from = "foo"
    [[tests.outputs.conditions]]
      type = "vrl"
      source = """
        assert_eq!(.message, "Sorry, I'm busy this week Cecil")
      """

  # And we add a new output without conditions for inspecting
  # a new transform
  [[tests.outputs]]
    extract_from = "bar"
```

How many tests you add is at your discretion, but you probably don't need to
test every single transform. We recommend every four transforms, except during a
full moon when you should test every two just to be sure.

## Organizing Configs

Building configs is only the beginning. Once it's built you need to make sure
pesky meddlers don't ruin it. The best way to keep on top of that is to break
large configs down into smaller more manageable pieces.

With Vector you can split a config down into as many files as you like and run
them all as a larger topology:

```bash
# These three examples run the same two configs together:
vector -c ./configs/foo.toml -c ./configs/bar.toml
vector -c ./configs/*.toml
vector -c ./configs/foo.toml ./configs/bar.toml
```

If you have a large chain of components it's a good idea to break them out into
individual files, each with its own unit tests.

## Splitting Configs

If your components start to be used in multiple configuration files, having a
dedicated place to define them can become interesting.

With Vector you can define a component configuration inside a component type folder.

Let's take an example with the following configuration file:

```toml title="vector.toml"
[sources.syslog]
type = "syslog"
address = "0.0.0.0:514"
max_length = 42000
mode = "tcp"

[transforms.change_fields]
type = "remap"
inputs = ["syslog"]
source = """
.new_field = "some value"
"""

[sinks.stdout]
type = "console"
inputs = ["change_fields"]
target = "stdout"
encoding.codec = "json"
```

We can extract the `syslog` source in the file `/etc/vector/sources/syslog.toml`

```toml title="syslog.toml"
type = "syslog"
address = "0.0.0.0:514"
max_length = 42000
mode = "tcp"
```

The `change_fields` transform in the file `/etc/vector/transforms/change_fields.toml`

```toml title="change_fields.toml"
type = "remap"
inputs = ["syslog"]
source = """
.new_field = "some value"
"""
```

And the `stdout` sink in the file `/etc/vector/sinks/stdout.toml`

```toml title="stdout.toml"
type = "console"
inputs = ["change_fields"]
target = "stdout"
```

And for Vector to look for the configuration in the component type related folders,
you need to start it using the `--config-dir` argument as follows.

```bash
vector --config-dir /etc/vector
```

## Updating Configs

Sometimes it's useful to update Vector configs on the fly. If you find yourself
tinkering with a config that Vector is already running you can prompt it to
reload the changes you've made by sending it a `SIGHUP` signal.

If you're running Vector in environments where it's not possible to issue
`SIGHUP` signals you can instead run it with the `--watch-config` flag and it'll
automatically gobble up changes whenever the file is written to.

[docs.about.concepts]: /docs/about/concepts/
[docs.setup.quickstart]: /docs/setup/quickstart/
[guides.unit-testing]: /guides/level-up/unit-testing/
