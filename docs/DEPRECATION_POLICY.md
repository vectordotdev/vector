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

### Security changes and critical bugs

Security fixes and critical bug fixes may change or remove existing behavior without prior notice, regardless of the
normal deprecation process. This includes changes that alter default configurations, disable insecure options, tighten
input validation, or otherwise restrict previously-allowed behavior in order to address a vulnerability or critical
bug, as well as removals introduced without a prior deprecation announcement. Such changes will be noted in the release
notes and we will do our best to provide an upgrade guide, but will not necessarily follow the standard migration window.

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

- Adding a [`deprecation.d/`](../deprecation.d/) fragment that lists the deprecation on the release page and on the
  always-current [deprecations index](https://vector.dev/deprecations/), including migration guidance. The release's
  upgrade guide may also call out the deprecation when it warrants a richer treatment than the fragment provides.
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

- Recording the removal as an enacted entry in the deprecations data (via `cargo vdev deprecation enact`), which moves
  the entry from the active list to the past-deprecations section on the [deprecations index](https://vector.dev/deprecations/)
  and surfaces it on the release page. The release's upgrade guide may also call out the removal under Breaking Changes
  when it warrants a richer treatment.

When possible, Vector will error at start-up when a removed configuration option or feature is used.

[configuration]: https://vector.dev/docs/reference/configuration/

## Process

When introducing a deprecation into Vector, the pull request introducing the deprecation should:

- **Add a deprecation fragment** to [`deprecation.d/`](../deprecation.d/) following the format in
  [`deprecation.d/README.md`](../deprecation.d/README.md). Set `deprecated_since` to the current release version.
  Use the fragment body for the full migration guide (rationale, before/after examples, links). Then run
  `cargo vdev deprecation generate` to regenerate `website/data/deprecations.json` and commit both files.
  Run `cargo vdev deprecation show` to preview, and `cargo vdev deprecation check` to validate.
  The fragment itself is the announcement; no separate changelog fragment is required (an announcement is not a
  change, so it does not belong in `changelog.d/`). The fragment is rendered on the release page in the
  Deprecation Announcements section and on the [deprecations index](https://vector.dev/deprecations/).
- Add a deprecation note to the component docs if applicable. Typically, this means adding `deprecation: "description of the deprecation"`
  to the cue file or `#[configurable(deprecated = "use <alternative> instead")]` to the parameter.
- For a component that is being renamed, remove the documentation page for the old name and add a new one for the new
  name. Add an alias so the old name redirects. The title of the new name should be appended with the text
  `(formerly OldName)`.
- Add a `WARN`-level log message starting with the word `DEPRECATION` if Vector detects the deprecated configuration
  or feature being used (when possible).

### Breaking changes require a prior announcement

A breaking change (any PR with a `type="breaking"` changelog fragment, or a removal of a deprecated feature) should
normally have been announced in an earlier release via a `deprecation.d/` fragment. Reviewers should ask the contributor
to land the announcement first, then come back to ship the removal after the migration window has passed (see the
[Policy](#policy) section for the minimum window).

The exception is the one described in [Security changes and critical bugs](#security-changes-and-critical-bugs): a
security issue or critical bug may justify shipping a breaking change without a prior announcement. Call that out
explicitly in the PR description so reviewers can apply the exception consciously rather than by oversight.

When removing a deprecation in a subsequent release, the pull request should:

- Mark the change as breaking by including `!` in the title after the type/scope.
- Remove the deprecation from the component documentation.
- Add a breaking changelog fragment in [`changelog.d`](../changelog.d/README.md). Enactment is the actual breaking
  change (the feature stops working), so it belongs in the release notes' Breaking changes section alongside other
  breaking changes. The Past Deprecations section is the lifecycle view, answering a different question for a
  different reader.
- Run `cargo vdev deprecation enact <slug> --version <removed-in-version>` and commit the result. This records the
  removal in `website/data/deprecations.json` and deletes the original fragment in one step.
