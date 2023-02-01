---
date: "2021-11-18"
title: "Elasticsearch sink config changes and deprecations"
description: ""
authors: ["fuchsnj"]
pr_numbers: []
release: "0.18.0"
hide_on_release_notes: false
badges:
  type: "deprecation"
---

Some fields of the elasticsearch sink config are deprecated and scheduled to be removed in 0.19.0

You may view the most recent config options here:
https://vector.dev/docs/reference/configuration/sinks/elasticsearch/

| Deprecated Field   | New Field             |
| -----------        | -----------           |
| `mode = normal`    | `mode = bulk`         |
| `host`             | `endpoint`            |
| `bulk_action`      | `bulk.action`         |
| `index`            | `bulk.index`          |
| `headers`          | `request.headers`     |

