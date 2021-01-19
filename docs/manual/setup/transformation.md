---
title: Transformation
description: Use Vector to transform observability data
---

Vector provides several [transforms][docs.transforms] that you can use to
modify your observability as it passes through your Vector
[topology][docs.topology].

## Vector Remap Language

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

[docs.lua]: /docs/reference/transforms/lua
[docs.topology]: /docs/about/under-the-hood/architecture/topology-model
[docs.transforms]: /docs/reference/transforms
[docs.wasm]: /docs/reference/transforms/wasm
[urls.lua]: https://www.lua.org
[urls.wasm]: https://webassembly.org
