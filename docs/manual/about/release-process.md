---
title: Release process
description: Vector's release process. Covering cadence and compatibility guarantees.
---

Here you can find information about how to track Vector's releases, how often
we release, compatibility guarantees, and more.

## How to track releases

There are a few different ways to track when there is a new release:

* Follow [@vectordotdev](https://twitter.com/vectordotdev) on Twitter.
* Heading to our [Discord server](https://discord.gg/EXXRbYCq) and watching the
	`#announcements` channel.
* Going to our [Github repository](https://github.com/timberio/vector) and
	using the "watch" feature. Click "Custom" and then "Releases" to only be
	notified for new releases.
* If you are using [one of our package
	repositories](https://cloudsmith.io/~timber/repos/vector/packages/) you should
	be able to see the update available when updating your package lists.

## Release cadence

We aim to:

* Release a stable version of Vector every 6 weeks.
* Release patch fixes as needed to fix high-priority bugs and regressions.
* Release daily builds representing the latest state of Vector for feedback.

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

* Allow users to beta test unreleased Vector features and provide us with
  feedback
* Run blackbox integration tests against released artifacts

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
