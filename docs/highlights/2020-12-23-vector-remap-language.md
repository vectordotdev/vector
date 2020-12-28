---
last_modified_on: "2020-12-23"
$schema: ".schema.json"
title: "Introducing Vector Remap Language"
description: "A lean, fast, and expressive language for transforming Vector observability data."
author_github: "https://github.com/lucperkins"
featured: true
pr_numbers: []
release: "0.12.0"
hide_on_release_notes: false
tags: ["type: featured", "domain: remap", "transform: remap"]
---

**Vector Remap Language** (VRL for short) is a language for transforming observability data in
Vector. VRL is:

* **Simple**, with just enough syntactic constructs to get the job done.
* **Expressive**, enabling you to perform operations as complex as you need.
* **Focused**, with a set of [functions][docs] tailored to fit observability use cases.
* **Fast**, because it's implemented in Rust, which means no garbage collection and a tight coupling with Vector's internal data models.

## VRL example

This TOML configuration example shows how you can use VRL in a Vector topology:

```toml
[sources.logs]
type = "syslog"
address = "0.0.0.0:9000"
mode = "tcp"

[transforms.cleanup]
type = "remap"
inputs = ["logs"]
source = """
# Convert the event to JSON
. = parse_json(.)
# Remove a superfluous field and trim the payload size
del(.unnecessary_field)
# Remove whitespace and convert to lower case
.message = strip_whitespace(downcase(.message))
# Prevent other systems from sensitive information
.credit_card = redact(.credit_card, filters = ["pattern"], redactor = "full", patterns = [/[0-9]{16}/])
# Add a UNIX timestamp
.timestamp = format_timestamp(now(), format = "%s")
"""

[sinks.console]
type = "console"
inputs = ["cleanup"]
encoding.codec = "json"
```

As you can see from the `cleanup` transform, VRL enables you to quickly accomplish exactly what you
need to and is highly readable and built for collaboration. This example is just a small taste of
[what is possible][docs] with the language.

## Why Vector Remap Language?

We built VRL because the two existing types of Vector transforms—"static" transforms like
[`remove_fields`][remove_fields] and runtime transforms like [WebAssembly][wasm] and [Lua]—have
drawbacks significant enough that we needed to provide Vector users with a better path forward.

The static transforms are inflexible, not very expressive, and often tricky to configure. They work
fine if the exact one you need is available, but if not you previously needed to opt for a runtime
transform. These transforms offer you the power of full programming languages

## Further reading

If your interest in VRL is now piqued, we recommend checking out these resources:

* The [VRL announcement post][post] on the Vector blog
* The [VRL documentation][docs]
* VRL [examples]

[docs]: https://vector.dev/docs/reference/remap
[examples]: https://vector.dev/docs/reference/transforms/remap#examples
[jq]: https://stedolan.github.io/jq
[lua]: https://vector.dev/docs/reference/transforms/lua
[post]: https://vector.dev/blog/vector-remap-language
[remove_fields]: https://vector.dev/docs/reference/transforms/remove_fields
[wasm]: https://vector.dev/docs/reference/transforms/wasm
