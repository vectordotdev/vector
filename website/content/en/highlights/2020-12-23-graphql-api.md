---
date: "2020-12-23"
title: "The GraphQL API for Vector"
description: "View Vector metrics and explore Vector topologies using GraphQL"
authors: ["lucperkins"]
pr_numbers: []
release: "0.12.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["api"]
---

Vector now has a [GraphQL] API that you can use for a variety of purposes:

* To view Vector's internal metrics
* To view metadata about the Vector instance
* To explore configured Vector topologies

We have plans to enhance the API in the coming releases by enabling you to, for
example, re-configure Vector via the API.

## How to use it

The GraphQL API for Vector is **disabled by default**. We want to keep Vector's
behavior as predictable and secure as possible, so we chose to make the feature
opt-in. To enable the API, add this to your Vector config:

```toml
[api]
enabled = true
address = "127.0.0.1:8686" # optional. Change IP/port if required
```

## Read more

For a more in-depth look at the API, check out:

* The recent [announcement post][post] for the API from esteemed Vector engineer [Lee Benson][lee].
* Our [official documentation]
* The [Rust code][code] behind the API

[code]: https://github.com/vectordotdev/vector/tree/master/src/api
[docs]: https://vector.dev/docs/reference/api
[graphql]: https://graphql.org
[lee]: https://github.com/LeeBenson
[post]: https://vector.dev/blog/graphql-api
