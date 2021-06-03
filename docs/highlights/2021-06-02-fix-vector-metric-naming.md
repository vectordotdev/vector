---
last_modified_on: "2021-06-02"
$schema: ".schema.json"
title: "Telemetry units for duration metrics fixed"
description: "A few telemetry metric names incorrectly had `nanoseconds_total` in their name when they are actually `seconds`"
author_github: "https://github.com/jszwedko"
pr_numbers: [7373]
release: "0.14.0"
hide_on_release_notes: false
tags: ["type: breaking change", "domain: metrics"]
---

The following internal histogram metrics, accessible via the `internal_metrics` source were incorrectly suffixed with
`nanoseconds_total` to indicate their unit as `nanoseconds` when they were in-fact representing `seconds`:

- `request_duration_nanoseconds`
- `collect_duration_nanoseconds`

These have been renamed to:

- `request_duration_seconds`
- `collect_duration_seconds`

## Upgrade Guide

If you were consuming these metrics, you will need to update any dashboards or
queries to use their new name.
