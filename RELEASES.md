# Releases

This document covers Vector's releases and the relevant aspect for Vector users.

<!-- MarkdownTOC autolink="true" style="ordered" indent="   " -->

1. [Channels](#channels)
   1. [Stable channel](#stable-channel)
   1. [Nightly channel](#nightly-channel)
1. [Tracking](#tracking)
   1. [Stable channel](#stable-channel-1)
   1. [Nightly channel](#nightly-channel-1)
1. [Downloading](#downloading)
1. [Cadence](#cadence)
   1. [Stable channel](#stable-channel-2)
   1. [Nightly channel](#nightly-channel-2)
1. [Guarantees](#guarantees)
1. [FAQ](#faq)
   1. [Which release type should I be using?](#which-release-type-should-i-be-using)

<!-- /MarkdownTOC -->

## Channels

### Stable channel

The stable release channel includes official Vector releases with a semantic
version.

### Nightly channel

The nightly channel is released nightly, based off of the current state of the
[`master` branch]. No guarantees are made with this branch. It may include
experimental or breaking changes.

## Tracking

### Stable channel

* Go to our [GitHub repository] and click the "watch" button in the top right.
  Click "Custom" and then "Releases" to only be notified for new releases.
  See the [GitHub subscription docs] for more info.
* Subscribe to the [Vector public calendar], release events are added.
* Follow [@vectordotdev] on Twitter.
* Head to our [chat], watch the `#announcements` channel, and configure
  notifications accordingly.
* If you are using a [package manager], you should be able to see the update
  available when updating your package lists.

### Nightly channel

Releases will appear in our [nightly artifact list] every night.

## Downloading

Please head over to Vector's [download page].

## Cadence

### Stable channel

* **Every 6 weeks**
* Release patch fixes as needed to fix high-priority bugs and regressions from the last major or minor release
* Release daily builds representing the latest state of Vector for feedback

### Nightly channel

* **Every night**

## Guarantees

Please see the [versioning policy].

## FAQ

### Which release type should I be using?

We always appreciate early feedback on Vector as we are developing it to help
ensure the highest quality releases.

If you are able to, running a nightly release of Vector allows you to
test out unreleased features and fixes and provide feedback to guide our
development. We aim to keep nightly as stable as possible through integration
testing, but there will occasionally be issues that slip through and are fixed
for the next nightly release. For example, you could choose to run the nightly
version in your development environments and save stable for production.
Otherwise, the stable release is your best bet.

[Vector public calendar]: https://calendar.vector.dev
[chat]: https://chat.vector.dev
[package manager]: https://vector.dev/docs/setup/installation/package-managers/
[download page]: https://vector.dev/download/
[nightly artifact list]: https://packages.timber.io/vector/nightly/
[@vectordotdev]: https://twitter.com/vectordotdev
[GitHub repository]: https://github.com/vectordotdev/vector
[GitHub subscription docs]: https://docs.github.com/en/github/managing-subscriptions-and-notifications-on-github/managing-subscriptions-for-activity-on-github/viewing-your-subscriptions
[`master` branch]: https://github.com/vectordotdev/vector/tree/master
[versioning policy]: https://github.com/vectordotdev/vector/blob/master/VERSIONING.md
