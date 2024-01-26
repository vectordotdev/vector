---
date: "2021-08-23"
title: "`wasm` transform to be removed from Vector"
description: "Removing experimental WASM support from Vector"
authors: ["jszwedko"]
pr_numbers: []
release: "0.16.0"
hide_on_release_notes: false
badges:
  type: "deprecation"
  domains: ["extensions"]
---

In `v0.10.0` we released experimental WASM support via the `wasm` transform to
allow transforms to be written in Rust. While this idea shows promise for
extensions in Vector, we have not been able to invest the time into it to make
it a first-class feature of Vector and so are removing it until such time. We
may revisit adding this back in the future,

This component is deprecated as of `v0.16.0` and will be removed in `v0.17.0`.

The primary purpose of this is removal to reduce maintenance burden and avoid
pushing users to use a transform for extensions that has a poor user experience
and poor performance.

In its place, the `lua` transform can be used to accomplish most custom tasks
the `wasm` transform would have been used for.

Are you currently using the `wasm` transform? We would like to hear about your
use-cases to ensure we accommodate them through other Vector features. Please
leave a comment on this [GitHub
issue](https://github.com/vectordotdev/vector/issues/8036).
