---
date: "2024-11-07"
title: "Exclusive Route Transform"
description: "Introducing the exclusive route transform"
authors: [ "binarylogic" ]
pr_numbers: [ 21707 ]
release: "0.43.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: [ "transforms" ]
---

The `ExclusiveRoute` transform splits an event stream into multiple sub-streams based on user-defined conditions. Each event will only be
routed to a single stream. This transforms complements the existing [Route transform][docs.transforms.route].

[docs.transforms.route]: https://vector.dev/docs/reference/configuration/transforms/route/
