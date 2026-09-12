# Debian: `/etc/vector/vector.yaml` is a conffile again, sample configs move to `/usr/share` {#debian-conffiles}

## Summary

`/etc/vector/vector.yaml` is now explicitly declared as a Debian `conf-files` entry, so
`dpkg` preserves local edits across upgrades instead of silently overwriting them (a
regression from #18718). The file it ships is an inert placeholder with no active
sources or sinks -- it does not run the `demo_logs` pipeline, consistent with 0.56.0's
removal of that noisy default.

The bundled sample configs, previously installed to `/etc/vector/examples/`, now install
to `/usr/share/vector/examples/` instead. They are reference material, not
admin-managed configuration, so they should not be treated as conffiles -- which they
implicitly were merely by living under `/etc`.

RPM packaging is unaffected by this change.

## Migration

If you referenced the bundled sample configs at `/etc/vector/examples/` (in scripts,
documentation, or tooling), update those references to `/usr/share/vector/examples/`.

No action is required for `/etc/vector/vector.yaml` itself. If you already had a
hand-created file there from before this change (when the package did not own that
path), it is preserved across the upgrade: the package's maintainer scripts back it up
before `dpkg` unpacks the new conffile default and restore it immediately after, so
existing content is never overwritten by the placeholder.

authors: yash1262
