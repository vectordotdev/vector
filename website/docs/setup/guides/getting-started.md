---
title: Getting Started Guide
sidebar_label: Getting Started
description: Getting started with Vector
---

Vector is a simple beast to tame, in this guide we'll send an
[event][docs.data-model#event] through it and touch on some basic concepts.

## 1. Install Vector

If you haven't already, install Vector. Here's a script for the lazy:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh
```

Or [choose your preferred installation method][docs.installation].

## 2. Configure it

Vector runs with a [configuration file][docs.configuration] that tells it which
components to run and how they should interact. Let's create one that simply
pipes a [`stdin` source][docs.sources.stdin] to a
[`stdout` sink][docs.sinks.console]:

import CodeHeader from '@site/src/components/CodeHeader';

<CodeHeader fileName="vector.toml" />

```toml
[sources.foo]
    type = "stdin"

[sinks.bar]
    inputs = ["foo"]
    type = "console"
    encoding = "text"
```

Every component within a Vector config has an identifier chosen by you. This
allows you to specify where a sink should gather its data from (using the
`inputs` field).

That's it for our first config, now pipe an event through it:

```bash
echo '172.128.80.109 - Bins5273 656 [2019-05-03T13:11:48-04:00] "PUT /mesh" 406 10272' | vector --config ./vector.toml
```

Your input event will get echoed back (along with some service logs) unchanged:

```text
... some logs ...
172.128.80.109 - Bins5273 656 [2019-05-03T13:11:48-04:00] "PUT /mesh" 406 10272
```

That's because the raw input text of our source was captured internally within
the field `message`, and the `text` encoding option of our sink prints the raw
contents of `message` only.

If you expected something interesting to happen then that's on you. The text
came out unchanged because we didn't ask Vector to change it, let's remedy that.
Exit Vector by pressing `ctrl+c`.

import Alert from '@site/src/components/Alert';

<Alert type="info">

Hey, kid, if you want to see something cool try setting `encoding = "json"` in
the sink config.

</Alert>

## 3. Transform an event

Nothing in this world is ever good enough for you, why should events be any
different?

Vector makes it easy to mutate events into a more (or less) structured format
with [transforms][docs.transforms]. Let's parse our log into a structured format
by capturing named regular expression groups with a
[`regex_parser` transform][docs.transforms.regex_parser].

A config can have any number of a transforms and it's entirely up to you how
they are chained together. Similar to sinks, a transform requires you to specify
where its data comes from. When a sink is configured to accept data from a
transform the pipeline is complete.

Let's place our new transform in between our existing source and sink. We are
also going to change the [encoding][docs.sinks.console#encoding] of our sink in
order to print the full event structure:

<CodeHeader fileName="vector.toml" />

```toml
[sources.foo]
    type = "stdin"

# Structure the data
[transforms.apache_parser]
    inputs = ["foo"]
    type = "regex_parser"
    field = "message"
    regex = '^(?P<host>[\w\.]+) - (?P<user>[\w]+) (?P<bytes_in>[\d]+) \[(?P<timestamp>.*)\] "(?P<method>[\w]+) (?P<path>.*)" (?P<status>[\d]+) (?P<bytes_out>[\d]+)$'

[sinks.bar]
    inputs = ["apache_parser"]
    type = "console"
    encoding = "json"
```

And pipe the same event again through it:

```bash
echo '172.128.80.109 - Bins5273 656 [2019-05-03T13:11:48-04:00] "PUT /mesh" 406 10272' | vector --config ./vector.toml
```

Oh snap! This time we get something like:

```text
... some logs ...
{"status":"406", "bytes_out":"10272", "path":"/mesh", "method":"PUT", "host":"172.128.80.109", "user":"Bins5273", "bytes_in":"656", "timestamp":"2019-05-03T13:11:48-04:00"}
```

Firstly, our `message` field has been parsed out into structured fields.
Secondly, we now see every field of the event printed to `stdout` by our sink in
JSON format because we set `encoding = "json"`.

Exit Vector again by pressing `ctrl+c`.

Next, try experimenting by adding more [transforms][docs.transforms] to your
pipeline before moving onto the next guide.


[docs.configuration]: /docs/setup/configuration/
[docs.data-model#event]: /docs/about/data-model/#event
[docs.installation]: /docs/setup/installation/
[docs.sinks.console#encoding]: /docs/reference/sinks/console/#encoding
[docs.sinks.console]: /docs/reference/sinks/console/
[docs.sources.stdin]: /docs/reference/sources/stdin/
[docs.transforms.regex_parser]: /docs/reference/transforms/regex_parser/
[docs.transforms]: /docs/reference/transforms/
