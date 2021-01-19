---
title: Transformation
description: Use Vector to transform observability data
---

Vector provides several [transforms][docs.transforms] that you can use to modify
your observability as it passes through your Vector [topology][docs.topology].

## Vector Remap Language

Let's jump straight into an example of using Vector Remap Language (VRL for
short) to modify some data. We'll create a simple topology with three
components:

1. A [`generator`][docs.generator] source produces random [Syslog][urls.syslog]
   messages at a rate of 10 per second.
2. A [`remap`][docs.remap] transform uses VRL to parse the Syslog lines into
   named fields (`severity`, `timestamp`, etc.).
3. A [`console`][docs.console] sink pipes the output of the topology to stdout,
   so that we can see the results on the timeline.

This configuration defines that topology:

```toml title="vector.toml"
[sources.logs]
  type = "generator"
  format = "syslog"
  interval = 0.1

[transforms.modify]
  type = "remap"
  inputs = ["logs"]
  source = '''
    # Parse Syslog input. The "!" means that the script should abort on error.
    . = parse_syslog!(.message)
  '''

[sinks.out]
  type = "console"
  inputs = ["modify"]
  encoding.codec = "json"
```

> Although we're using [TOML][urls.toml] for the configuration here, Vector also
> supports JSON and YAML.

To start Vector using this topology:

```bash
vector --config-toml /etc/vector/vector.toml
```

You should see lines like this emitted via stdout:

```json
{"appname":"devankoshal","facility":"daemon","hostname":"some.us","message":"#hugops to everyone who has to deal with this","msgid":"ID486","procid":5265,"severity":"notice","timestamp":"2021-01-19T18:16:40.027Z"}
```

So far, Vector is *parsing* the Syslog data but we're not yet really *modifying*
it. So let's update the `source` script of our `remap` transform to make some
ad hoc transformations:

```toml
[transforms.modify]
  type = "remap"
  inputs = ["logs"]
  source = '''
    . = parse_syslog!(.message)

    # Convert the timestamp to a Unix timestamp, aborting on error
    .timestamp = to_unix_timestamp!(.timestamp)

    # Remove the "facility" field
    del(.facility)

    # Replace the "msgid" field with a unique ID
    .msgid = uuid_v4()

    # If the log message contains the phrase "Great Scott!", set the new field
    # "critical" to true, otherwise set it to false. If the "contains" function
    # errors, log the error (instead of aborting the script, as above).
    if (is_critical, err = contains(.message, "Great Scott!"); err != null) {
      log(err, level: "error")
    }

    if is_critical {
      .critical = true
    } else {
      .critical = false
    }
  '''
```

## Runtime transforms

If VRL doesn't cover your use case—and that should happen rarely—Vector also
offers two **runtime transforms** that you can use to modify logs and
metrics flowing through your topology:

* The [`wasm`][docs.wasm] transform enables you to run compiled
  [WebAssembly][urls.wasm] code using a Wasm runtime inside of Vector.
* The [`lua`][docs.lua] transform enables you to run [Lua][urls.lua] code
  that you can include directly in your Vector configuration

Both of the runtime transforms provide maximal flexibility because they enable
you to use full-fledged programming languages right inside of Vector. But we
recommend using these transforms only when truly necessary, for several reasons:

1. The runtime transforms make it all too easy to write transforms that are
   slow, error prone, and hard to read.
2. Both require you to add a coding/testing/debugging workflow to using Vector,
   which is worth the effort when truly necessary but best avoided if possible.

[docs.console]: /docs/reference/transforms/console
[docs.generator]: /docs/reference/transforms/generator
[docs.lua]: /docs/reference/transforms/lua
[docs.remap]: /docs/reference/transforms/remap
[docs.topology]: /docs/about/under-the-hood/architecture/topology-model
[docs.transforms]: /docs/reference/transforms
[docs.wasm]: /docs/reference/transforms/wasm
[urls.lua]: https://www.lua.org
[urls.syslog]: https://en.wikipedia.org/wiki/Syslog
[urls.toml]: https://toml.io
[urls.wasm]: https://webassembly.org
