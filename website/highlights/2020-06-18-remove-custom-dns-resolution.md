---
$schema: "/.meta/.schemas/highlights.json"
title: "Remove custom DNS resolution"
description: "<fill-in>"
author_github: "https://github.com/a_hoverbear"
hide_on_release_notes: false
pr_numbers: [2812]
release: "0.10.0"
tags: ["type: breaking change"]
---

In [#2635](https://github.com/timberio/vector/issues/2635) we discussed removing the custom DNS server.

## Upgrade Guide

Make the following changes in your `vector.toml` file:

```diff title="vector.toml"
-  dns_servers = [...]
```