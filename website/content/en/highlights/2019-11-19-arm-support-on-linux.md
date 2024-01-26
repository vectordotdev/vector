---
date: "2020-03-31"
title: "ARMv7 and ARM64 Support on Linux"
description: "These architectures are widely used in embedded devices and servers"
authors: ["binarylogic"]
pr_numbers: [1054, 1292]
release: "0.6.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["platforms"]
  platforms: ["arm64", "arm7"]
aliases: ["/blog/arm-support-on-linux"]
---

Vector now supports [ARM architectures][urls.arm] on the Linux platform! These
architectures are widely used in embedded devices and recently started to get
traction on servers. To get started, you can follow the installation
instructions for your preferred method:

- [DPKG][docs.package-managers.dpkg] (select the ARM tab)
- [RPM][docs.package-managers.rpm] (select the ARM tab)
- [Docker][docs.platforms.docker] (select the ARM tab)
- [From archives][docs.manual.from-archives]
- [Or, download the files directly][pages.releases]

<!--more-->

## Fully-static without dependencies

It's worth noting that Vector ships a fully static binary without dependencies.
This makes installation as simple as copying the Vector binary onto
your machine. There are no dependencies to install or environment changes
required.

## DPKG, RPM, and Docker support

In addition to providing archives for these architectures, we went the extra
mile to ensure [DPKG][docs.package-managers.dpkg],
[RPM][docs.package-managers.rpm], and [Docker][docs.platforms.docker] support
them as well.

## Usecases

If you're wondering how this benefits you, here are a couple of popular use
cases:

### AWS performance improvements and cost savings

An interesting usecase for ARM support are the [new AWS `M6g`, `C6g`, and `R6g`
instances][urls.aws_arm_g2_announcement]. These instances are based on Amazon's
ARM-based Graviton2 processors. Amazon claims they "deliver up to 40% improved
price/performance over current generation `M5`, `C5`, and `R5` instances".

### Raspbian, IoT, and embedded devices

ARM architectures are widely used on IoT devices. Vector is the perfect
candidate for resource constrained environments like this, especially given
Vector's superior memory efficiency.

## The case for Vector

If you're using a vendor-based data collector it's likely they lack support
for the ARM architectures, especially the new ARM64 architecture. This limits
your flexibility, and in the case of AWS, can have direct cost implications.
Supporting these platforms, properly, is Vector's core competency.

[docs.manual.from-archives]: /docs/setup/installation/manual/from-archives/
[docs.package-managers.dpkg]: /docs/setup/installation/package-managers/dpkg/
[docs.package-managers.rpm]: /docs/setup/installation/package-managers/rpm/
[docs.platforms.docker]: /docs/setup/installation/platforms/docker/
[pages.releases]: /releases/
[urls.arm]: https://en.wikipedia.org/wiki/ARM_architecture
[urls.aws_arm_g2_announcement]: https://aws.amazon.com/about-aws/whats-new/2019/12/announcing-new-amazon-ec2-m6g-c6g-and-r6g-instances-powered-by-next-generation-arm-based-aws-graviton2-processors/
