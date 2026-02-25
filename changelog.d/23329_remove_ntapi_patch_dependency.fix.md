Remove the `[patch.crates-io]` override for `ntapi` and use the crates.io release.
To keep Windows (`default-msvc`) builds working, `host_metrics` CPU/host/load collectors are now behind a non-Windows feature gate.
authors: Trighap52
