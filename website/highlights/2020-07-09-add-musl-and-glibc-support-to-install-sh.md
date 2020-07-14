---
last_modified_on: "2020-07-14"
$schema: "/.meta/.schemas/highlights.json"
title: "glibc enhancements"
description: "If your Linux uses glibc, Vector will too."
author_github: "https://github.com/hoverbear"
hide_on_release_notes: false
pr_numbers: [2969,2518]
release: "0.10.0"
tags: ["type: breaking change", "domain: operations"]
---

As a result of some recent profiling and benchmarking, we determined that builds of Vector targetting `x86_64-unknown-linux-gnu` outperformed builds targetting `x86_64-unknown-linux-musl` under most usage scenarios on supported operating systems.

After some deliberation, we've opted to package and ship glibc builds where it's safe. While this change means Vector is no longer fully static on operating systems which use glibc, it should provide a better user experience.

## Measures we've take to safeguard your deployments

* This change **only affects** x86 platforms.
* This change **affects** `.deb` and `.rpm` packages.
* This change **affects** Vectors installed via `https://sh.vector.dev`.
* This change **does not affect** any [archives][urls.vector_download] you may already be using. We now publish archives with the `gnu` prefix that contain glibc builds. *Musl builds are untouched as `musl` still.*
* This change **does not affect** non-Linux platforms.

If you've fought with glibc before, you've probably got a burning question:

**Which version do we support? 🕵️‍♀️**

**Don't worry. We have your back. 🤜🤛** We're using a base of CentOS 7, which means new Vector glibc builds will support all the way back to **glibc 2.17**. If that's still too new for your machines, please keep using the Musl builds. (Also, [Let us know!][urls.new_bug_report])


## Upgrade Guide

You should not need to do anything. If you are using a normal, reccommended method of installing Vector, you should not experience issues.

[**If you do. That's a bug. 🐞 We squash bugs. Report it.**][urls.new_bug_report]

If you're provisioning Vector, the best way to make sure you get the most up to date stable version is to run this:

```bash title="provision_vector.sh"
curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh -s -- -y
```

If you don't need the latest and greatest, **check your official distribution repository.** Some distributions, such as [NixOS][urls.nixos], have official Vector packages. You can also find Vector packages in the official [FreeBSD][urls.freebsd] repositories.

Interested in packaging Vector for your OS? We are too. [Why don't you let us know it's important to you?][urls.new_feature_request]

[urls.freebsd]: https://www.freebsd.org/
[urls.new_bug_report]: https://github.com/timberio/vector/issues/new?labels=type%3A+bug
[urls.new_feature_request]: https://github.com/timberio/vector/issues/new?labels=type%3A+new+feature
[urls.nixos]: https://nixos.org/
[urls.vector_download]: https://vector.dev/releases/latest/download/
