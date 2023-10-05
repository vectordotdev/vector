---
title: Structuring, Shaping, and Transforming Data
short: Transforming
description: Use Vector to parse, structure, shape, and transform observability data
author_github: lucperkins
domain: processing
weight: 1
tags: ["transform", "observability", "level up", "guides", "guide"]
---

{{< requirement >}}
Before you begin, this guide assumes the following:

* You understand the [basic Vector concepts][concepts]
* You understand [how to set up a basic pipeline][pipeline]

[concepts]: /docs/about/concepts
[pipeline]: /docs/setup/quickstart
{{< /requirement >}}

Vector provides multiple [transforms][docs.transforms] that you can use to
modify your observability data as it passes through your Vector
[topology][docs.architecture.pipeline-model].

The transform that you will likely use most often is the [`remap`][docs.transforms.remap]
transform, which uses a single-purpose data transformation language called
[Vector Remap Language][docs.vrl] (VRL for short) to define event
transformation logic. VRL has several features that should make it your first
choice for transforming data in Vector:

* It offers a wide range of observability-data-specific
  [functions][docs.vrl.functions] that map directly to observability use cases.
* It's built for the very specific use case of working with Vector logs and
  metrics, which means that it has no extraneous functionality, its data model
  maps directly to Vector's internal data model, and its performance is comparable
  to native [Rust][urls.rust] performance.
* The VRL compiler built into Vector performs several compile-time checks to
  ensure that your VRL code is sound, meaning no dead code, no unhandled errors,
  and no type mismatches.

In cases where VRL doesn't fit your use case, Vector also offers a [Lua runtime
transform](#lua-runtime-transform) that offer a bit more flexibility than VRL but
also come with downsides (listed below) that should always be borne in mind.

## Transforming data using VRL

Let's jump straight into an example of using VRL to modify some data. We'll
create a simple topology consisting of three components:

1. A [`demo_logs`][docs.sources.demo_logs] source produces random [Syslog][urls.syslog]
   messages at a rate of 10 per second.
2. A [`remap`][docs.transforms.remap] transform uses VRL to parse incoming Syslog lines
   into named fields (`severity`, `timestamp`, etc.).
3. A [`console`][docs.sinks.console] sink pipes the output of the topology to stdout,
   so that we can see the results on the command line.

This configuration defines that topology:

```yaml title="vector.yaml"
sources:
  logs:
    type: demo_logs
    format: syslog
    interval: 0.1

transforms:
  modify:
    type: remap
    inputs:
      - logs
    source: |
      # Parse Syslog input. The "!" means that the script should abort on error.
      . = parse_syslog!(.message)

sinks:
  out:
    type: console
    inputs:
      - modify
    encoding:
      codec: json
```

{{< info >}}
Although we're using [YAML][urls.yaml] for the configuration here, Vector also
supports [TOML][urls.toml] and [JSON][urls.json].

[urls.toml]: https://github.com/toml-lang/toml
[urls.yaml]: https://yaml.org
[urls.json]: https://www.json.org/json-en.html
{{< /info >}}

To start Vector using this topology:

```bash
vector --config /etc/vector/vector.yaml
```

You should see lines like this emitted via stdout (formatted for readability
here):

```json
{
  "appname": "authsvc",
  "facility": "daemon",
  "hostname": "acmecorp.biz",
  "message": "#hugops to everyone who has to deal with this",
  "msgid": "ID486",
  "procid": 5265,
  "severity": "notice",
  "timestamp": "2021-01-19T18:16:40.027Z"
}
```

So far, we've gotten Vector to *parse* the Syslog data but we're not yet
*modifying* that data. So let's update the `source` script of our `remap`
transform to make some ad hoc transformations:

```yaml
transforms:
  modify:
    type: remap
    inputs:
      - logs
    source: |
      . = parse_syslog!(.message)

      # Convert the timestamp to a Unix timestamp, aborting on error
      .timestamp = to_unix_timestamp!(.timestamp)

      # Remove the "facility" and "procid" fields
      del(.facility)
      del(.procid)

      # Replace the "msgid" field with a unique ID
      .msgid = uuid_v4()

      # If the log message contains the phrase "Great Scott!", set the new field
      # "critical" to true, otherwise set it to false. If the "contains" function
      # errors, log the error (instead of aborting the script, as above).
      if (is_critical, err = contains(.message, "Great Scott!"); err != null) {
        log(err, level: "error")
      }

      .critical = is_critical
```

A few things to notice about this script:

* Any errors thrown by VRL functions must be handled. Were we to neglect to
  handle the potential error thrown by the `parse_syslog` function, for example,
  the VRL compiler would provide a very specific warning and Vector wouldn't
  start up.
* VRL has language constructs like variables, `if` statements, comments, and
  logging.
* The `.` acts as a sort of "container" for the event data. `.` by itself refers
  to the root event, while you can use [paths] like `.foo`,
  `.foo[0]`, `.foo.bar`, `.foo.bar[0]`, and so on to reference subfields, array
  indices, and more.

If you stop and restart Vector, you should see log lines like this (again
reformatted for readability):

```json
{
  "appname": "authsvc",
  "hostname": "acmecorp.biz",
  "message": "Great Scott! We're never gonna reach 88 mph with the flux capacitor in its current state!",
  "msgid": "4e4437b6-13e8-43b3-b51e-c37bd46de490",
  "severity": "notice",
  "timestamp": 1611080200,
  "critical": true
}
```

And that's it! We've successfully created a Vector topology that transforms
every event that passes through it. If you'd like to know more about VRL, we
recommend checking out the following documentation:

* A full list listing of [VRL functions][docs.vrl.functions]
* [VRL examples][docs.vrl.examples]
* [VRL expressions][docs.vrl.expressions], which describes things VRL's syntax
  and type system in great detail

## Lua runtime transform

If VRL doesn't cover your use case—and that should happen rarely—Vector also
offers a [`lua`][docs.lua] **runtime transform** that you can use instead of
VRL. It enables you to run [Lua][urls.lua] code that you can include directly in
your Vector configuration

The `lua` transform provides maximal flexibility because they enable you to use
a full-fledged programming language right inside of Vector. But we recommend
using it only when truly necessary, for several reasons:

1. The `lua` transform makes it all too easy to write scripts that are slow,
   error prone, and hard to read.
2. It requires you to add a coding/testing/debugging workflow to using Vector,
   which is worth the effort if there's no other way to satisfy your use case
   but best avoided if possible.
3. It imposes a performance penalty vis-à-vis VRL.

[docs.architecture.pipeline-model]: /docs/about/under-the-hood/architecture/pipeline-model/
[docs.lua]: /docs/reference/configuration/transforms/lua/
[docs.setup.quickstart]: /docs/setup/quickstart/
[docs.sinks.console]: /docs/reference/configuration/sinks/console/
[docs.sources.demo_logs]: /docs/reference/configuration/sources/demo_logs/
[docs.transforms.remap]: /docs/reference/configuration/transforms/remap/
[docs.transforms]: /docs/reference/configuration/transforms/
[docs.vrl.examples]: /docs/reference/vrl/examples/
[docs.vrl.expressions]: /docs/reference/vrl/expressions/
[docs.vrl.functions]: /docs/reference/vrl/functions/
[docs.vrl]: /docs/reference/vrl
[paths]: /docs/reference/vrl/expressions/#path
[urls.lua]: https://www.lua.org
[urls.new_feature_request]: https://github.com/vectordotdev/vector/issues/new?labels=type%3A+new+feature
[urls.rust]: https://www.rust-lang.org/
[urls.syslog]: https://en.wikipedia.org/wiki/Syslog
