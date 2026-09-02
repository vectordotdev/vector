---
date: "2021-06-02"
title: "Switching to the system allocator"
description: "Vector has switched from using jemalloc to the system allocator"
authors: ["jszwedko"]
pr_numbers: [6781]
release: "0.14.0"
hide_on_release_notes: false
badges:
  type: "announcement"
  domains: ["performance"]
---

Since version 0.2.0, Vector has used [jemalloc] as its memory allocator on *nix-based OSes. As part of
some ongoing performance work, we've made the decision to switch to the system allocator provided by the platform
Vector is deployed on. This will be either the [GNU Allocator][gnu_allocator] for `glibc`-based builds (like
Debian) or the [`musl` allocator][musl_allocator] for `musl`-based builds (like Alpine Linux).

In environments where Vector has access to multiple CPUs, we recommend using the `glibc`-based builds as, at the time of
writing, [we have observed that the GNU allocator performs
better][performance] when running on multiple threads.

If you are installing one of the packages or release assets listed on the [release page][0_14],
then you will be getting a `glibc` build. `musl` builds are available as direct
[x86_64][0_14_musl_x86_64], [AArch64][0_14_musl_aarch64], and [ARMv7][0_14_musl_armv7] archives.

If you are running Vector in Docker, we recommend using the `v0.14.0-distroless-libc` image for a light-weight Vector
image; however the `v0.14.0-debian` image will also be using the GNU Allocator. The `v0.14.0-alpine` and
`v0.14.0-static` images use `musl` as `glibc` is not available there.

This change was made largely because we had insufficient evidence and motivation to use anything other than the system
allocator which is a sensible default otherwise. As we have a better understanding of Vector's allocation profile, it is
likely we will revisit this decision.

[0_14]: /releases/0.14.0
[0_14_musl_aarch64]: https://install.datadoghq.com/vector/0.14.0/vector-0.14.0-aarch64-unknown-linux-musl.tar.gz
[0_14_musl_armv7]: https://install.datadoghq.com/vector/0.14.0/vector-0.14.0-armv7-unknown-linux-musleabihf.tar.gz
[0_14_musl_x86_64]: https://install.datadoghq.com/vector/0.14.0/vector-0.14.0-x86_64-unknown-linux-musl.tar.gz
[gnu_allocator]: https://www.gnu.org/software/libc/manual/html_node/The-GNU-Allocator.html
[jemalloc]: https://github.com/jemalloc/jemalloc
[musl_allocator]: https://musl.libc.org/releases.html
[performance]: https://github.com/vectordotdev/vector/issues/1985#issuecomment-670667972
