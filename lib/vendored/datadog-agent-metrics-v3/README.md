# datadog-agent-metrics-v3

This crate is vendored from [Saluki](https://github.com/DataDog/saluki) at tag
[`1.7.2`](https://github.com/DataDog/saluki/tree/1.7.2/lib/datadog-agent/metrics-v3)
(commit `24fa65a2b42c7bfd69abebdeb771d656512e0586`). Saluki does not publish it to crates.io.

The source files in `src/` are unchanged copies of the upstream files. The code is licensed under
Apache-2.0. Refer to `LICENSE` and `NOTICE`. The local `rustfmt.toml` and `clippy.toml` files
copy the applicable Saluki settings, so that Vector's formatting and lint checks accept the
upstream code without changes.

To update the crate, copy `lib/datadog-agent/metrics-v3/src` from the new Saluki release into
`src/` and update the tag and commit in this file and in `Cargo.toml`. Do not make local changes
to the source files. Send changes upstream to Saluki.
