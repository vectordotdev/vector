---
date: "2020-03-31"
title: "New Prometheus Source"
description: "Scrape prometheus metrics with Vector"
authors: ["binarylogic"]
pr_numbers: [1264]
release: "0.7.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["sources"]
  sources: ["prometheus"]
aliases: ["/blog/prometheus-source"]
---

We love [Prometheus][urls.prometheus], but we also love [options](https://www.mms.com/en-us/shop/single-color)
and so we've added a [`prometheus_scrape` source][docs.sources.prometheus] to let you
send Prometheus format metrics anywhere you like.

<!--more-->

This was an important feat for Vector because it required us to mature our
metrics data model and tested our interoperability between metrics sources.

To use it simply add the source config and point it towards the hosts you wish
to scrape:

```toml
[sources.my_source_id]
  type = "prometheus_scrape"
  hosts = ["http://localhost:9090"]
  scrape_interval_secs = 1
```

For more guidance get on the [reference page][docs.sources.prometheus].

## Why?

We believe the most common use cases for this source will be backups and
migration, if you have an interesting use case we'd [love to hear about it][urls.vector_chat].

[docs.sources.prometheus]: /docs/reference/configuration/sources/prometheus_scrape
[urls.prometheus]: https://prometheus.io/
[urls.vector_chat]: https://chat.vector.dev
