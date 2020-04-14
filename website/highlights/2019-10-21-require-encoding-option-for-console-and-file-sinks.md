---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "Require `encoding` option for console and file sinks"
description: "The `encoding` option is now required for these sinks"
author_github: "https://github.com/binarylogic"
pr_numbers: [1033]
release: "0.6.0"
importance: "low"
tags: ["type: breaking change", "domain: sinks", "sink: console", "sink: file"]
---

The dynamic `encoding` concept in Vector was confusing users, so we've made
it required an explicit. Simply add `encode = "json"` to your `console` and
`file` sinks.



