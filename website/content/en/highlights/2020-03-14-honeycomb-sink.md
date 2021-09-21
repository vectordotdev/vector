---
date: "2020-07-13"
title: "New Honeycomb Sink"
description: "Sink logs to the Honeycomb logging service"
authors: ["binarylogic"]
pr_numbers: [1665]
release: "0.9.0"
hide_on_release_notes: true
badges:
  type: "new feature"
  domains: ["sinks"]
  sinks: ["honeycomb"]
---

For you [Honeycomb][urls.honeycomb] fans we have a new
[`honeycomb` sink][docs.sinks.honeycomb]. Keep an eye on
[PR#1991][urls.pr_1991], which will introduce a new `transaction` transform.
This transform is designed to produce "canonical" events. These are flattened,
wide events that represent an entire transaction, the concept that Honeycomb
is built upon. Vector + Honeycomb = 👯.

[docs.sinks.honeycomb]: /docs/reference/configuration/sinks/honeycomb/
[urls.honeycomb]: https://honeycomb.io
[urls.pr_1991]: https://github.com/vectordotdev/vector/pull/1991
