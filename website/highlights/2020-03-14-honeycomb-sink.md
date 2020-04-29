---
last_modified_on: "2020-04-13"
title: "New Honeycomb Sink"
description: "Sink logs to the Honeycomb logging service"
author_github: "https://github.com/binarylogic"
pr_numbers: [1665]
release: "0.9.0"
hide_on_release_notes: true
tags: ["type: new feature", "domain: sinks", "sink: honeycomb"]
---

For you [Honeycomb][urls.honeycomb] fans we have a new
[`honeycomb` sink][docs.sinks.honeycomb]. Keep an eye on
[PR#1991][urls.pr_1991], which will introduce a new `transaction` transform.
This tranformed is designed to produce "canoncial" events. These are flattened,
wide events that represent an entire transaction, the concept that Honeycomb
is built upon. Vector + Honeycomb = 👯.


[docs.sinks.honeycomb]: /docs/reference/sinks/honeycomb/
[urls.honeycomb]: https://honeycomb.io
[urls.pr_1991]: https://github.com/timberio/vector/pull/1991
