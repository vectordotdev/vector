---
date: "2020-04-20"
title: "Improved Shutdown"
description: "A faster and more reliable shutdown process"
authors: ["binarylogic"]
pr_numbers: [1994]
release: "0.9.0"
hide_on_release_notes: false
badges:
  type: "enhancement"
  domains: ["topology"]
---

A graceful shutdown process is often a problematic achievement in software.
It takes a lot of coordination to ensure interconnected components shut down
in the right order and in time. In the context of Vector, it's critically
essential because it prevents data loss in real-world practice. As a result,
this is one area we want to get right. [PR#1994][urls.pr_1994] introduces a
clear shutdown strategy that moves the shutdown logic into Vector's topology,
simplifying the shutdown implementation for each component. This change ensures
that shutdown is easily and correctly implemented. The result is not only faster
shutdowns but much higher confidence that components in the future will reliably
shutdown.

[urls.pr_1994]: https://github.com/vectordotdev/vector/pull/1994
