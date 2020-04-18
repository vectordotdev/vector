---
last_modified_on: "2020-03-31"
$schema: "/.meta/.schemas/highlights.json"
title: "New Filter Transform"
description: "Filter and route your logs based on defined conditions"
author_github: "https://github.com/binarylogic"
pr_numbers: [2088]
release: "nightly"
hide_on_release_notes: false
tags: ["type: new feature", "domain: transforms", "transform: filter"]
---

We love [Prometheus][urls.prometheus], but we also love [options](https://www.mms.com/en-us/shop/single-color)
and so we've added a [`prometheus` source][docs.sources.prometheus] to let you
send Prometheus format metrics anywhere you like.

<!--truncate-->

This was an important feat for Vector because it required us to mature our
metrics data model and tested our interoperability between metrics sources
To use it simply add the source config and point it towards the hosts you wish
to scrape:

```toml
[sources.my_source_id]
  type = "prometheus"
  hosts = ["http://localhost:9090"]
  scrape_interval_secs = 1
```

For more guidance get on the [reference page][docs.sources.prometheus].

## Why?

We believe the most common use cases for this source will be backups and
migration, if you have an interesting use case we'd [love to hear about it][urls.vector_chat].


[docs.sources.prometheus]: /docs/reference/sources/prometheus/
[urls.prometheus]: https://prometheus.io/
[urls.vector_chat]: https://chat.vector.dev
