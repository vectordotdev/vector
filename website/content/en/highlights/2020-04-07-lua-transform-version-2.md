---
date: "2020-03-31"
title: "Lua Transform v2"
description: "The next iteration of our Lua transform."
authors: ["binarylogic"]
pr_numbers: [2126]
release: "0.9.0"
hide_on_release_notes: false
badges:
  type: enhancement
  domains: ["sources"]
  sources: ["vector"]
---

v2 of our [`lua` transform][docs.transforms.lua] has been released! This is a
complete overhaul that provides a new and improved API, better data processing
ergonomics, and faster processing. Specific improvements include:

1. Events are [represented as Lua tables][docs.transforms.lua#event-data-model] with proper type conversion.
2. Introduction of [hooks][docs.transforms.lua#hooks] to maintain global state.
3. Introduction of [timers][docs.transforms.lua#timers] to facilitate timed flushing. Useful for aggregations.
4. The ability to accept and work with metric events in addition to log events.

This raises the bar in terms of capabilities, which is important! Lua is often
used as an escape hatch when Vector's native transforms are not expressive
enough.

{{< info >}}
Did you know we're also [working on a WASM integration][urls.pr_2006] 👀

[urls.pr_2006]: https://github.com/vectordotdev/vector/pull/2006
{{< /info >}}

## Get Started

{{< jump "/docs/reference/configuration/transforms/lua" >}}
{{< jump "/guides/advanced/custom-aggregations-with-lua" >}}
{{< jump "/guides/advanced/parsing-csv-logs-with-lua" >}}
{{< jump "/guides/advanced/merge-multiline-logs-with-lua" >}}

And for the curious, check out [Vector's Lua RFC][urls.rfc].

[docs.transforms.lua#hooks]: /docs/reference/configuration/transforms/lua/#hooks
[docs.transforms.lua#event-data-model]: /docs/reference/configuration/transforms/lua/#event-data-model
[docs.transforms.lua#timers]: /docs/reference/configuration/transforms/lua/#timers
[docs.transforms.lua]: /docs/reference/configuration/transforms/lua/
[urls.rfc]: https://github.com/vectordotdev/vector/blob/master/rfcs/2020-03-06-1999-api-extensions-for-lua-transform.md
