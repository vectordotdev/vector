---
date: "2020-04-13"
title: "Rename `line` field to `message` in `splunk_hec` source"
description: "This change ensures the `splunk_hec` source conforms to Vector's schema"
authors: ["binarylogic"]
pr_numbers: [1457]
release: "0.7.0"
hide_on_release_notes: false
badges:
  type: "breaking change"
  domains: ["sources"]
  sources: ["splunk_hec"]
---

The `splunk_hec` source now emits events with a `message` key instead of a
`line` key. This can be renamed via the [global `log_schema`
options][docs.reference.configuration.global-options#log_schema].

## Upgrade Guide

There are no changes you need to make. Just be aware that your events will
no longer have a `line` field.

[docs.reference.configuration.global-options#log_schema]: /docs/reference/configuration/global-options/#log_schema
