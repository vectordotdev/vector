# Versioning

This document covers Vector's versioning and what it means as a user of Vector.

**Please note, Vector is currently in its pre-1.0 phase and quickly approaching
1.0. Minor version increments can introduce breaking changes during this phase.
Please see the [what to expect](#what-to-expect) section for more info.**

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Convention](#convention)
1. [Public API](#public-api)
   1. [Areas that *are* covered](#areas-that-are-covered)
      1. [Intended for *public* consumption](#intended-for-public-consumption)
      1. [Intended for *private* consumption](#intended-for-private-consumption)
   1. [Areas that are *NOT* covered](#areas-that-are-not-covered)
1. [Releases](#releases)
   1. [Release channels](#release-channels)
      1. [Stable channel](#stable-channel)
      1. [Nightly channel](#nightly-channel)
   1. [Release tracking](#release-tracking)
      1. [Stable channel](#stable-channel-1)
      1. [Nightly channel](#nightly-channel-1)
   1. [Release downloading](#release-downloading)
   1. [Release cadence](#release-cadence)
      1. [Stable channel](#stable-channel-2)
      1. [Nightly channel](#nightly-channel-2)
1. [FAQ](#faq)
   1. [Which release type should I be using?](#which-release-type-should-i-be-using)
   1. [How does Vector treat patch and minor versions?](#how-does-vector-treat-patch-and-minor-versions)
   1. [How does Vector treat major versions \(breaking changes\)?](#how-does-vector-treat-major-versions-breaking-changes)
   1. [How does Vector treat pre-1.0 versions?](#how-does-vector-treat-pre-10-versions)
   1. [Release cadence](#release-cadence-1)

<!-- /MarkdownTOC -->

## Convention

Vector adheres to the [Semantic Versioning 2.0] convention. In summary:

* Versions follow the `MAJOR.MINOR.PATCH` format (i.e., `2.5.1`)
* `PATCH` increments only when backward compatible bug fixes are introduced.
* `MINOR` increments only when new, backward compatible functionality is introduced.
* `MAJOR` increments if any backwards incompatible changes are introduced.
* Pre `1.0` (major version `0`) is for initial development and `MINOR` version bumps can introduce breaking changes.

## Public API

Semantic Versioning hinges on Vector's defintion of "public API". By nature of
Vector - a tool that collects, processes, and routes data from disparate systems
- it has a very large public surface area. It's not immediately obvious which
parts are covered under our versioning contract and how they're covered. This
section aims to remove all ambiguity in this area.

### Areas that *are* covered

The following Vector areas are covered in Vector's definition of public API.

#### Intended for *public* consumption

The follow Vector areas are inteded for *public* consumption (consumption by
anything other than Vector itself). Backward incompatble changes will trigger
a major version increment.

* [CLI]
  * The root [`vector` command] and its input/output.
  * The [`vector validate` subcommand] and its input/output.
* [GraphQL API]

#### Intended for *private* consumption

The following Vector areas are inteded for *private* consumption (consumption by
Vector only). Backward incompatble changes will trigger a major version
increment only if Vector itself is not compatbile with previous versions.

* [Configuration schema]
* [Data directory] and it's contents

### Areas that are *NOT* covered

The following Vector areas are *not* covered in Vector's definition of Public
API. Breaking changes in these areas will *not* trigger a major version
increment.

* [CLI]
  * The [`vector generate` subcommand] and its input/output.
  * The [`vector graph` subcommand] and its input/output.
  * The [`vector help` subcommand] and its input/output.
  * The [`vector list` subcommand] and its input/output.
  * The [`vector tap` subcommand] and its input/output.
  * The [`vector top` subcommand] and its input/output.
  * The [`vector vrl` subcommand] and its input/output.
* [Installation workflows]

## Releases

### Release channels

#### Stable channel

The stable release channel includes official Vector releases with a semantic
version.

#### Nightly channel

The nightly channel is released nightly, based off of the current state of the
[`master` branch]. No guarantees are made with this branch. It may include
experimental or breaking changes.

### Release tracking

#### Stable channel

* Go to our [Github repository] and click the "watch" button in the top right.
  Click "Custom" and then "Releases" to only be notified for new releases.
  See the [Github subscriptiond docs] for more info.
* Subscribe to the [Vector public calendar], release events are added.
* Follow [@vectordotdev] on Twitter.
* Head to our [chat], watch the `#announcements` channel, and configure
  notifications accordingly.
* If you are using a [package manager], you should be able to see the update
  available when updating your package lists.

#### Nightly channel

Releases will appear in our [nightly artifact list] every night.

### Release downloading

Please head over to Vector's [download page].

### Release cadence

#### Stable channel

* **Every 6 weeks**
* Release patch fixes as needed to fix high-priority bugs and regressions
* Release daily builds representing the latest state of Vector for feedback

#### Nightly channel

* **Every night**

## FAQ

### Which release type should I be using?

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

### How does Vector treat patch and minor versions?

As defined by [Semantic Versioning], you can expect no breaking changes. Users
will be able to seamlessly upgrade without any action.

### How does Vector treat major versions (breaking changes)?

Major versions break backward compatibility. Vector takes breaking changes very
seriously. We understand that Vector is a critical part of your infrastructure
and breaking changes introduce downtime. We will make every effort necessary
to avoid them. If we introduce them we will make the upgrade process as painless
as possible. Every major release will come with a single, step-by-step upgrade
guide in the [release notes].

### How does Vector treat pre-1.0 versions?

As defined by [Semantic Versioning]:

> major version zero (0.y.z) is for initial development. Anything MAY change at
> any time.

And while this is true to the spec, Vector takes breaking changes *very*
seriously during this phase. What's outlined in the
[major versions](##major-versions-breaking-changes) section still holds true
here. Each minor release bump will include an upgrade guide in the
[release notes] if necessary.

### Release cadence

[@vectordotdev]: https://twitter.com/vectordotdev
[chat]: https://chat.vector.dev
[CLI]: https://vector.dev/docs/reference/cli/
[configuration schema]: https://vector.dev/docs/reference/configuration/
[data directory]: https://vector.dev/docs/reference/configuration/global-options/#data_dir
[Github repository]: https://github.com/timberio/vector
[Github subscriptiond docs]: https://docs.github.com/en/github/managing-subscriptions-and-notifications-on-github/managing-subscriptions-for-activity-on-github/viewing-your-subscriptions
[GraphQL API]: https://vector.dev/docs/reference/api/
[Installation workflows]: https://vector.dev/docs/setup/installation/
[`master` branch]: https://github.com/timberio/vector/tree/master
[nightly artifact list]: https://packages.timber.io/vector/nightly/
[package manager]: https://vector.dev/docs/setup/installation/package-managers/
[release notes]: https://vector.dev/releases/
[Semantic Versioning]: https://semver.org/spec/v2.0.0.html
[`vector` command]: https://vector.dev/docs/reference/cli/#vector
[`vector generate` subcommand]: https://vector.dev/docs/reference/cli/#generate
[`vector graph` subcommand]: https://vector.dev/docs/reference/cli/#graph
[`vector help` subcommand]: https://vector.dev/docs/reference/cli/#help
[`vector list` subcommand]: https://vector.dev/docs/reference/cli/#list
[Vector public calendar]: https://calendar.vector.dev
[`vector tap` subcommand]: https://vector.dev/docs/reference/cli/#tap
[`vector top` subcommand]: https://vector.dev/docs/reference/cli/#top
[`vector validate` subcommand]: https://vector.dev/docs/reference/cli/#validate
[`vector vrl` subcommand]: https://vector.dev/docs/reference/cli/#vrl
