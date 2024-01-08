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

- Add a note to the Deprecations section of the upgrade guide for the next release with a description and
  directions for transitioning if applicable.
- Copy the same note from the previous step, to a changelog fragment, with type="deprecation". See the changelog
  fragment [README.md](../changelog.d/README.md) for details.
- Add a deprecation note to the docs. Typically, this means adding `deprecation: "description of the deprecation"`
  to the `cue` data for the option or feature. If the `cue` schema does not support `deprecation` for whatever you
  are deprecating yet, add it to the schema and open an issue to have it rendered on the website.
- For a component that is being renamed, the documentation page for the old name of the component is removed and a
  new page is added for the new name. An alias is added so the old name will redirect to the new name. The title of
  the new name will be appended with the text `(formerly OldName)`.
- Add a log message to Vector that is logged at the `WARN` level starting with the word `DEPRECATION` if Vector detects
  the deprecated configuration or feature being used (when possible).
- Add the deprecation to [DEPRECATIONS.md](DEPRECATIONS.md) to track migration (if applicable) and removal

When removing a deprecation in a subsequent release, the pull request should:

- Indicate that it is a breaking change by including `!` in the title after the type/scope
- Remove the deprecation from the documentation
- Add a note to the Breaking Changes section of the upgrade guide for the next release with a description and directions
  for transitioning if applicable.
- Copy the same note from the previous step, to a changelog fragment, with type="breaking". See the changelog
  fragment [README.md](../changelog.d/README.md) for details.
- Remove the deprecation from [DEPRECATIONS.md](DEPRECATIONS.md)
