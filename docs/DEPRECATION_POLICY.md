# Deprecations in Vector

This document covers Vector's deprecation policy and process.

In the course of Vector's development it can be necessary to deprecate configuration and (rarely) features to keep
Vector maintainable and its configuration interface consistent for users. To avoid breaking compatibility abruptly, we
follow the following deprecation policy.

## Policy

Vector will retain deprecated configuration or features for at least one minor version (this will transition to one
major version when Vector hits 1.0).

This means that deprecations will be eligible for removal in the next minor release after they are announced; however,
we will typically aim to support deprecations for a longer time period depending on their development maintenance
burden. For example, a deprecation announced in `v0.16.0` would be eligible to be removed in `v0.17.0` but may be
removed later in `v0.20.0`.

Exceptions can be made for deprecations related to security issues or critical bugs. These may result in removals being
introduced in a release without being announced in a prior release.

### Examples

Examples of possible deprecations in Vector:

- Removal or rename of a configuration option
- Removal or rename of a metric
- Removal or rename of a component
- Removal of a feature

## Lifecycle of a deprecation

A deprecation goes through three stages: Deprecation, Migration, and Removal. These are described below.

### Deprecation

A configuration option or feature in Vector is marked as deprecated.

When this happens, we will notify by:

- Listing the deprecation in the Deprecations section of the upgrade guide for the release the deprecation was
  introduced in. This will include instructions on how to transition if applicable.
- Adding a deprecation note to the [documentation site][configuration] alongside the configuration or feature being
  deprecated.
- When possible, output a log at the `WARN` level if Vector detects deprecated configuration or features being used
  on start-up, during `vector validate`, or at runtime. This log message will lead with the text `DEPRECATED` to
  make it easy to filter for.

### Migration

Users will have 1 or more minor releases to migrate away from using the deprecation using the instructions provided in
the deprecation notice.

### Removal

A deprecated configuration option or feature in Vector is removed.

When this happens, we will notify by:

- Listing the removal in the Breaking Changes section of upgrade guide for that release. This will include directions on
  how to transition if applicable.

When possible, Vector will error at start-up when a removed configuration option or feature is used.

[configuration]: https://vector.dev/docs/reference/configuration/

## Process

When introducing a deprecation into Vector, the pull request introducing the deprecation should:

- **Add a deprecation fragment** to [`deprecation.d/`](../deprecation.d/) following the format in
  [`deprecation.d/README.md`](../deprecation.d/README.md). Set `deprecated_since` to the current release version.
  Use the fragment body for the full migration guide (rationale, before/after examples, links). Then run
  `cargo vdev deprecation generate` to regenerate `website/data/deprecations.json` and commit both files.
  Run `cargo vdev deprecation show` to preview, and `cargo vdev deprecation check` to validate.
- Add a changelog fragment with `type="deprecation"` ([`changelog.d/README.md`](../changelog.d/README.md)). A short
  one-line summary is sufficient — the deprecation fragment is the canonical migration guide and is rendered on the
  release page automatically.
- Add a deprecation note to the component docs. Typically, this means adding `deprecation: "description of the deprecation"`
  to the `cue` data for the option or feature. If the `cue` schema does not support `deprecation` for whatever you
  are deprecating yet, add it to the schema and open an issue to have it rendered on the website.
- For a component that is being renamed, remove the documentation page for the old name and add a new one for the new
  name. Add an alias so the old name redirects. The title of the new name should be appended with the text
  `(formerly OldName)`.
- Add a `WARN`-level log message starting with the word `DEPRECATION` if Vector detects the deprecated configuration
  or feature being used (when possible).

When removing a deprecation in a subsequent release, the pull request should:

- Mark the change as breaking by including `!` in the title after the type/scope.
- Remove the deprecation from the component documentation.
- Add a changelog fragment with `type="breaking"` ([`changelog.d/README.md`](../changelog.d/README.md)). A short
  one-line summary is sufficient — the enacted deprecation entry is the canonical record of what was removed.
- Run `cargo vdev deprecation enact <slug> --version <removed-in-version>` and commit the result. This records the
  removal in `website/data/deprecations.json` and deletes the original fragment in one step.
