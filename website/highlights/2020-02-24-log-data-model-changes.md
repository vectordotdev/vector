---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "Log Data Model Changes & Disk Buffers"
description: "We're bringing our log data model closer to JSON"
author_github: "https://github.com/binarylogic"
pr_numbers: [1836, 1898]
release: "0.8.0"
hide_on_release_notes: true
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
Unfortunately, this breaks disk buffer serialization which means you must
drain your disk bufffer before upgrading Vector.

## Upgrade Guide

1. Make sure Vector shuts down normally to ensure your disk buffers are fully
   drained.
2. That's it! Update Vector as usual.

Note, Vector will discard invalid disk buffer data, bad data will not prevent
Vector from starting.


[docs.data-model.log]: /docs/about/data-model/log/
