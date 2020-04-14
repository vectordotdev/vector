---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "Log Data Model Improvements"
description: "We're bringing our log data model closer to JSON"
author_github: "https://github.com/binarylogic"
pr_numbers: [1836, 1898]
release: "0.8.0"
importance: "low"
tags: ["type: breaking change", "domain: buffers", "event type: log"]
---

We are currently working to improve and optimize our [`log` data
model][docs.data-model.log]. Initial versions of this data model were
represented as a flat map for key access optimizations. This proved over time
to not be as helpful as we had hoped. As a result we are working to move our
data model to be as close to JSON as possible. This means:

1. `null` values are now supported in Vector's data model.
2. Nested fields are represented in an actual nested representation.

Both of these changes bring Vector's internal data model closer to JSON.

## What does this mean for you?

The data model changes break serialization and therefore any data in your
disk buffer will not be recoverable when Vector is upgraded. You should
ensure that your disk buffer is drained before upgrading Vector. This can
be achieved by allowing Vector to shut down normally (not forcibly killed).
That's it!

Note, Vector will discard invalid disk buffer data, bad data will not prevent
Vector from starting.


[docs.data-model.log]: /docs/about/data-model/log/
