# Patched quanta 0.12

This is `quanta` 0.12.6 from crates.io, at upstream commit
`0f81349c223854136113e634cf8dd6cd85569880` in
[metrics-rs/quanta](https://github.com/metrics-rs/quanta).
The crate archive SHA-256 is
`f3ab5a9d756f0d97bdc89019bd2e4ea098cf9cde50ee7564dde6b81ccc8f06c7`.
The MIT license is in `LICENSE`.

## Local changes

- Initialize `libc::timespec` with `Default` instead of a struct literal, so private
  padding fields on 32-bit musl targets do not prevent compilation. This backports
  the fix released in quanta 0.13.0 from
  [upstream PR 121](https://github.com/metrics-rs/quanta/pull/121).
- Add a monotonic-clock regression test.
- Disable publishing in the package manifest.
- Keep the upstream formatting and lint defaults with local configuration files.

The 0.12 API, feature flags, and dependencies are unchanged. The root Cargo patch
selects this copy for `metrics-util` and `moka`, which still use quanta 0.12.
Vector's existing quanta 0.13 dependency remains on the registry version.

The crate is excluded from the Vector workspace to preserve its upstream test
dependencies. Remove this copy and its Cargo patch when the fix is available in a
compatible upstream 0.12 release, or when no dependency requires quanta 0.12.
