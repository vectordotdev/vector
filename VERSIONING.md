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
1. [What to expect](#what-to-expect)
   1. [Patch and minor versions](#patch-and-minor-versions)
   1. [Major versions \(breaking changes\)](#major-versions-breaking-changes)
   1. [Pre 1.0 version](#pre-10-version)

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

## What to expect

As a Vector user you can expect the following for each version change.

### Patch and minor versions

As defined by [Semantic Versioning], you can expect no breaking changes. Users
will be able to seamlessly upgrade without any action.

### Major versions (breaking changes)

Major versions break backward compatibility. Vector takes breaking changes very
seriously. We understand that Vector is a critical part of your infrastructure
and breaking changes introduce downtime. We will make every effort necessary
to avoid them. If we introduce them we will make the upgrade process as painless
as possible. Every major release will come with a single, step-by-step upgrade
guide in the [release notes].

### Pre 1.0 version

As defined by [Semantic Versioning]:

> major version zero (0.y.z) is for initial development. Anything MAY change at
> any time.

And while this is true to the spec, Vector takes breaking changes *very*
seriously during this phase. What's outlined in the
[major versions](##major-versions-breaking-changes) section still holds true
here. Each minor release bump will include an upgrade guide in the
[release notes] if necessary.

[CLI]: https://vector.dev/docs/reference/cli/
[configuration schema]: https://vector.dev/docs/reference/configuration/
[data directory]: https://vector.dev/docs/reference/configuration/global-options/#data_dir
[GraphQL API]: https://vector.dev/docs/reference/api/
[Installation workflows]: https://vector.dev/docs/setup/installation/
[release notes]: https://vector.dev/releases/
[Semantic Versioning]: https://semver.org/spec/v2.0.0.html
[`vector` command]: https://vector.dev/docs/reference/cli/#vector
[`vector generate` subcommand]: https://vector.dev/docs/reference/cli/#generate
[`vector graph` subcommand]: https://vector.dev/docs/reference/cli/#graph
[`vector help` subcommand]: https://vector.dev/docs/reference/cli/#help
[`vector list` subcommand]: https://vector.dev/docs/reference/cli/#list
[`vector tap` subcommand]: https://vector.dev/docs/reference/cli/#tap
[`vector top` subcommand]: https://vector.dev/docs/reference/cli/#top
[`vector validate` subcommand]: https://vector.dev/docs/reference/cli/#validate
[`vector vrl` subcommand]: https://vector.dev/docs/reference/cli/#vrl
