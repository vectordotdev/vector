---
date: "2020-07-17"
title: "Leveraging glibc when possible"
description: "If your Linux uses glibc, Vector will too."
authors: ["hoverbear"]
hide_on_release_notes: false
pr_numbers: [2969, 2518]
release: "0.10.0"
badges:
  type: "performance"
  domains: ["operations"]
---

As a result of some recent profiling and benchmarking, we determined that builds of Vector targeting `x86_64-unknown-linux-gnu` outperformed builds targeting `x86_64-unknown-linux-musl` under most usage scenarios on supported operating systems.

Glibc builds may see performance improve by up to 20% over musl. For more information see
the [original issue][urls.vector_glibc_benchmarks]. We're investigating ways to improve our MUSL performance, too! Stay tuned to future releases.

After some deliberation, we've opted to package and ship glibc builds where it's safe. While this change means Vector is no longer fully static on operating systems which use glibc, it should provide a better user experience.

For the vast majority of users **no special action needs to be taken.**

## Measures we've take to safeguard your deployments

- This change **only affects** x86 platforms.
- This change **affects** `.deb` and `.rpm` packages and Vectors **installed via** `https://sh.vector.dev`.
- This change **does not affect** any [archives][urls.vector_download] you may already be using. We now publish archives
  with the `gnu` prefix that contain glibc builds. _Musl builds are untouched as `musl` still._
- This change **does not affect** non-Linux platforms.

If you've fought with glibc before, you've probably got a burning question:

> Which version do we support? 🕵️‍♀️

**Don't worry. We have your back. 🤜🤛** We're using a base of CentOS 7, which means new Vector glibc builds will support all the way back to **glibc 2.17** (released 2012-12-25). If that's still too new for your machines, please keep using the Musl builds. (Also, [Let us know!][urls.new_bug_report])

## Upgrade Guide

You **should not need to do anything**. If you are using a normal, recommended method of installing Vector, you should not experience issues.

[**If you do. That's a bug. 🐞 We squash bugs. Report it.**][urls.new_bug_report]

If you're provisioning Vector, the best way to make sure you get the most up to date stable version is to run this:

```bash title="provision_vector.sh"
curl --proto '=https' --tlsv1.2 -sSfL https://sh.vector.dev | bash -s -- -y
```

If you don't need the latest and greatest, **check your official distribution repository.** Some distributions, such as [NixOS][urls.nixos], have official Vector packages. You can also find Vector packages in the official [FreeBSD][urls.freebsd] repositories.

Interested in packaging Vector for your OS? We are too. [Why don't you let us know it's important to you?][urls.new_feature_request]

[urls.freebsd]: https://www.freebsd.org/
[urls.new_bug_report]: https://github.com/vectordotdev/vector/issues/new?labels=type%3A+bug
[urls.new_feature_request]: https://github.com/vectordotdev/vector/issues/new?labels=type%3A+new+feature
[urls.nixos]: https://nixos.org/
[urls.vector_download]: https://vector.dev/releases/latest/download/
[urls.vector_glibc_benchmarks]: https://github.com/vectordotdev/vector/issues/2313
