# Patched protoc-bin-vendored

This is `protoc-bin-vendored` 3.2.0 from crates.io, at upstream commit
`895c0433c3727a552970ce961e398e20e52d6353` in
[stepancheg/rust-protoc-bin-vendored](https://github.com/stepancheg/rust-protoc-bin-vendored).
The crate archive SHA-256 is
`d1c381df33c98266b5f08186583660090a4ffa0889e76c7e9a5e175f645a67fa`.
The MIT license is in `LICENSE`, copied from the same upstream commit.

## Local changes

- Return the path in `PROTOC` before selecting a bundled binary. This permits a
  native ARMv7 or musl compiler even when no compatible binary is bundled.
- Add a regression test that checks the override in a separate process.
- Disable publishing and register the regression test in the package manifest.
- Keep the upstream formatting and lint defaults with local configuration files.

The architecture-specific binary dependencies and the fallback selection are
unchanged. An unset `PROTOC` retains upstream behavior. `include_path()` is also
unchanged; it still selects bundled include files by host architecture.

The root Cargo patch selects this copy for existing transitive users. The crate
is excluded from the Vector workspace to preserve its upstream test dependencies.

Remove this copy and its Cargo patch when the upstream helper or its callers
support a configured native `protoc` without this change.
