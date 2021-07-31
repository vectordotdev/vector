# Releasing

This document will cover how to track Vector's releases, how often
we release, compatibility guarantees, how to release Vector, and more.

Vector adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) and
the release process is dependent on the version change.

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [How to track releases](#how-to-track-releases)
1. [Release cadence](#release-cadence)
   1. [Stable releases](#stable-releases)
   1. [Patch releases](#patch-releases)
   1. [Nightly releases](#nightly-releases)
1. [Compatibility guarantees](#compatibility-guarantees)
1. [Which version should I be using?](#which-version-should-i-be-using)
1. [How to release Vector](#how-to-release-vector)
   1. [Quick Start](#quick-start)
      1. [Patch Releases](#patch-releases-1)
      1. [Major/Minor Releases](#majorminor-releases)
   1. [Fixing Up a Release](#fixing-up-a-release)

<!-- /MarkdownTOC -->

## How to track releases

There are a few different ways to track when there is a new release:

- Follow [@vectordotdev](https://twitter.com/vectordotdev) on Twitter.
- Heading to our [Discord server](https://discord.gg/EXXRbYCq) and watching the
  `#announcements` channel.
- Going to our [Github repository](https://github.com/timberio/vector) and
  using the "watch" feature. Click "Custom" and then "Releases" to only be
  notified for new releases.
- If you are using [one of our package
  repositories](https://cloudsmith.io/~timber/repos/vector/packages/) you should
  be able to see the update available when updating your package lists.

## Release cadence

We aim to:

- Release a stable version of Vector every 6 weeks.
- Release patch fixes as needed to fix high-priority bugs and regressions.
- Release daily builds representing the latest state of Vector for feedback.

### Stable releases

Vector aims to do a stable feature release every 6 weeks (this is a minor
release in [semantic versioning parlance](https://semver.org/)).

We aim to keep this cadence regular, but there may be some variance due to
holidays, critical bug fixes, or other scheduling concerns.

While Vector is pre-1.0, a stable release is represented by a bump to the minor
version of Vector's version number. For example: `v0.11.1` to `v0.12.0`.

### Patch releases

Patch releases can be pushed out at any point to include regression and high
priority bug fixes. We evaluate at the end of every two weeks whether a patch
should be published for the current stable release, but can release outside of
this schedule depending on need.

By default, any bug fixes that are not regression fixes will be left for the
next stable release.

### Nightly releases

We release a nightly version of Vector every day that represents the latest
state of Vector. We aim to keep these as stable as possible, but they are
inherently less stable than patch releases. We offer these to:

- Allow users to beta test unreleased Vector features and provide us with
  feedback
- Run blackbox integration tests against released artifacts

These nightly releases can be downloaded via our [nightly downloads
page](https://vector.dev/releases/nightly/download/) or via our our [nightly
package
repositories](https://cloudsmith.io/~timber/repos/vector-nightly/packages/).

## Compatibility guarantees

We aim to keep Vector as backwards-compatible as possible, preferring
deprecation over breaking compatibility, but we will occasionally introduce
backwards-incompatible changes as we learn better ways of doing things.

While Vector is pre-1.0, we will only make backwards-incompatible changes in
minor releases (for example from `0.11.1` to `0.12.0`). We will not make
backwards-incompatible changes in point releases (for example from `0.11.0` to
`0.11.1`) unless there is a critical bug that must be addressed that requires
breaking compatibility to fix (this has never happened).

When there are backwards-incompatible changes in a release, they will always be
highlighted in our release notes under the "Breaking changes" heading (for
example, the [0.12.0 release breaking
changes](https://vector.dev/releases/0.12.0/#breaking-change-highlights)).

There are no guarantees of compatibility between nightly vector releases.

These compatibility guarantees will be revisited after 1.0 to adhere with
[semantic versioning](https://semver.org/).

## Which version should I be using?

We always appreciate early feedback on Vector as we are developing it to help
ensure the highest quality releases.

If you are able to, running the [nightly
release](https://vector.dev/releases/nightly/download/) of Vector allows you to
test out unreleased features and fixes and provide feedback to guide our
development. We aim to keep nightly as stable as possible through integration
testing, but there will occasionally be issues that slip through and are fixed
for the next nightly release. For example, you could choose to run the nightly
version in your development environments and save stable for production.
Otherwise, the stable release is your best bet.

## How to release Vector

The following section is targeted at maintainers! If you're simply using Vector,
it is probably not relevant.

### Quick Start

#### Patch Releases

1. Create a new branch from the latest `vMAJOR.MINOR.PATCH` tag. Ex: `git checkout -b v1.2.3 v1.2.2`
2. Make the appropriate changes/fixes.
3. Update the `version` key in [`/Cargo.toml`] and run `cargo build` to get the version bump in the `Cargo.lock` file.
4. Update the [`/CHANGELOG.md`] header to reflect the new version `vMAJOR.MINOR.PATCH - 2019-05-02`
5. Commit the changes above with message "Release vMAJOR.MINOR.PATCH"
6. Create a new tag named `vMAJOR.MINOR.PATCH`
7. Push the new tag
8. Delete the temporary branch you created.
9. [All done](https://i.giphy.com/media/3ohzdIvnUKKjiAZTSU/giphy.webp)

#### Major/Minor Releases

1. Switch to the `master` branch, this should be reflective of the new version's changes.
2. Update the `version` key in [`/Cargo.toml`] and run `cargo build` to get the version bump in the `Cargo.lock` file.
3. Update the [`/CHANGELOG.md`] header to reflect the new version `vMAJOR.MINOR.0 - 2019-05-02`
4. Commit the changes above with message `"Release vMAJOR.MINOR.PATCH"`
5. Create a new tag named `vMAJOR.MINOR.PATCH`
6. Push the new tag.
7. Update the [`/CHANGELOG.md`] header to reflect the new upcoming version `vNEW_MAJOR.NEW_MINOR-dev`
8. Commit the changes above with message `"Start vNEW_MAJOR.NEW_MINOR+1"`
9. [All done](https://i.giphy.com/media/3ohzdIvnUKKjiAZTSU/giphy.webp)

### Fixing Up a Release

If you tried to cut a release and the CI failed for some unexpected reason, you can follow these steps to recover:

1. Switch to the `v$VERSION` branch.
1. Delete the `v$VERSION` tag.
1. Cherry pick the commits directly to the branch.
1. Run `make release` while still on that branch.
1. Commit and tag accordingly.
1. Cherry pick that commit back to master so that the release is carried over.

[All done]: https://i.giphy.com/media/3ohzdIvnUKKjiAZTSU/giphy.webp
[`/Cargo.toml`]: /Cargo.toml
[`/CHANGELOG.md`]: /CHANGELOG.md
