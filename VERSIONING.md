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
1. [FAQ](#faq)
   1. [Which release type should I be using?](#which-release-type-should-i-be-using)
   1. [Release cadence](#release-cadence)

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
  * The root [`vector` command], its flags and exit code
  * The [`vector validate` subcommand], the flags exit codes
* [Data model]
  * As exposed in the source of the [`lua` transform]
* [GraphQL API]
* Telemetry
  * Vector's internal metrics as provided by the [`internal_metrics` source]
* [VRL]

#### Intended for *private* consumption

The following Vector areas are inteded for *private* consumption (consumption by
Vector only). Backward incompatble changes will trigger a major version
increment only if Vector itself is not compatbile with previous versions.

* [Configuration schema]
* [Data directory] and its contents
* [Data model]
  * As exposed in the output of the [`vector` sink]

### Areas that are *NOT* covered

The following Vector areas are *not* covered in Vector's definition of Public
API. Breaking changes in these areas will *not* trigger a major version
increment.

* [CLI]
  * The [`vector generate` subcommand]
  * The [`vector graph` subcommand]
  * The [`vector help` subcommand]
  * The [`vector list` subcommand]
  * The [`vector tap` subcommand]
  * The [`vector top` subcommand]
  * The [`vector vrl` subcommand]
* [Installation workflows]
* Telemetry
  * Vector's internal logs as provided through `STDOUT`, `STDERR`, and the [`internal_logs` source]

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

### Release cadence

[@vectordotdev]: https://twitter.com/vectordotdev
[chat]: https://chat.vector.dev
[CLI]: https://vector.dev/docs/reference/cli/
[configuration schema]: https://vector.dev/docs/reference/configuration/
[data directory]: https://vector.dev/docs/reference/configuration/global-options/#data_dir
[data model]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/
[Github repository]: https://github.com/timberio/vector
[GraphQL API]: https://vector.dev/docs/reference/api/
[Installation workflows]: https://vector.dev/docs/setup/installation/
[`internal_logs_` source]: https://vector.dev/docs/reference/configuration/sources/internal_logs/
[`internal_metrics` source]: https://vector.dev/docs/reference/configuration/sources/internal_metrics/
[`lua` transform]: https://vector.dev/docs/reference/configuration/transforms/lua/
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
[`vector` sink]: https://vector.dev/docs/reference/configuration/sinks/vector/
[`vector tap` subcommand]: https://vector.dev/docs/reference/cli/#tap
[`vector top` subcommand]: https://vector.dev/docs/reference/cli/#top
[`vector validate` subcommand]: https://vector.dev/docs/reference/cli/#validate
[`vector vrl` subcommand]: https://vector.dev/docs/reference/cli/#vrl
[VRL]: https://vector.dev/docs/reference/vrl/
