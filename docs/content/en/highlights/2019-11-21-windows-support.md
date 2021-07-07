---
date: "2020-03-31"
title: "Windows Support Is Here!"
description: "Bringing the performance and reliability of Vector to Windows"
authors: ["binarylogic"]
pr_numbers: [1205]
release: "0.6.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["platforms"]
  platforms: ["windows"]
aliases: ["/blog/windows-support"]
---

We're excited to announce that Vector can now be installed on Windows!
To get started, check out the [Windows installation instructions][docs.operating-systems.windows]
or head over to the [releases section][pages.releases] and download the
appropriate Windows archive. Just like on Linux, installation on Windows is
quick and easy. [Let us know what you think!][pages.community].

## First-class support

It's important to point out that Vector's support for Windows is _first-class_.
This is not a makeshift integration. Vector is written in [Rust][urls.rust],
and Windows is a [tier 1 platform][urls.rust_tier_1_platform], meaning the same
reliability and performance is brought to Windows as it is on Linux.

## No dependencies required

Beyond Rust's tier 1 support for Windows, Vector does not require the
installation of any dependencies. This makes installation as simple as copying
the Vector binary onto your machine. There are no additional DLL files to
install or environment changes required.

## Windows Installer Support

In addition to installing Vector through a [`.zip` archive][pages.releases],
we've included an [`.msi` installer][pages.releases] that makes installing
Vector even simpler. This can be done [through the command line][docs.package-managers.msi]:

```bat
msiexec /i vector-x64.msi /quiet
```

Or manually through the GUI installer:

![Vector Windows Installation](https://res.cloudinary.com/timber/image/upload/v1576161621/Vector%20Website/vector-windows-install.gif)

## An important step for Vector

A core tenet of Vector is that users should own and control their data; this
starts with data collection. So often we see organizations deploying many
(sometimes 10 or more) data collectors just because upstream and downstream
support is lacking. This is especially true when Windows environments are
involved. This leads to fragmentation in the tooling used to build out these
pipelines. Vector aims to solve this, and supporting Windows is a big step
towards accomplishing that vision.

[docs.operating-systems.windows]: /docs/setup/installation/operating-systems/windows/
[docs.package-managers.msi]: /docs/setup/installation/package-managers/msi/
[pages.community]: /community/
[pages.releases]: /releases/
[urls.rust]: https://www.rust-lang.org/
[urls.rust_tier_1_platform]: https://forge.rust-lang.org/release/platform-support.html#tier-1
