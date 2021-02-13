---
last_modified_on: "2020-01-12"
$schema: ".schema.json"
title: "Introducing Vector Remap Language"
description: "A lean, fast, and expressive language for transforming observability data."
author_github: "https://github.com/lucperkins"
featured: true
pr_numbers: []
release: "0.12.0"
hide_on_release_notes: false
tags: ["type: featured", "domain: remap"]
---

The Vector team is excited to announce the **Vector Remap Language**
(VRL for short). VRL is a purpose-built observability data mapping language
designed for high-volume processing. VRL is built on the following two
principles:

1. **Performance** - In addition to being built in Rust, and tightly integrated
   with Vector, VRL is carefully designed to prevent operators from writing
   slow scripts. This avoids performance footguns exposed through runtimes like
   Lua or Javascript.
2. **Safety** - Like Rust, safety is built throughout the entire language. VRL
   implements thoughtful limitations, compile-time checks, required error
   handling, and type safety. If a VRL expression compiles, you can have high
   cofidence it will work as expected in production.

[**Read the VRL announcement post →**][post]

## VRL example

This TOML configuration example shows how you can use VRL in a Vector topology:

```toml
[sources.logs]
type = "file"
include = ["/var/log/*.log"]

[transforms.cleanup]
type = "remap"
inputs = ["logs"]
source = '''
. = parse_syslog(.message)
.message = parse_json(.message)
.status = to_int(.status)
.duration = parse_duration(.duration)
.message = redact(.message, filters = ["pattern"], redactor = "full", patterns = [/[0-9]{16}/])
"""

[sinks.console]
type = "console"
inputs = ["cleanup"]
encoding.codec = "json"
```

As you can see from the `cleanup` transform, VRL enables you to quickly process
your data without the need to chain together many fundamental transforms or
pay the performance and safety cost of a full runtime like Lua or Javascript.

## Why Vector Remap Language?

We built VRL because the two existing types of Vector transforms—"static" transforms like
[`remove_fields`][remove_fields] and runtime transforms like [WebAssembly][wasm], [Lua],
and Javascript—have drawbacks significant enough that we needed to provide Vector users
with a better path forward.

## Further reading

If your interest in VRL is now piqued, we recommend checking out these resources:

* The [VRL announcement post][post] on the Vector blog
* The [VRL documentation][docs]
* VRL [examples]

[docs]: /docs/reference/vrl
[examples]: /docs/reference/transforms/remap#examples
[jq]: https://stedolan.github.io/jq
[lua]: /docs/reference/transforms/lua
[post]: /blog/vector-remap-language
[remove_fields]: /docs/reference/transforms/remove_fields
[wasm]: /docs/reference/transforms/wasm
