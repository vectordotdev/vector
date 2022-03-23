---
date: "2021-08-20"
title: "Improved defaults for sink rate limits"
description: "Removing artificial restrictions by default"
authors: ["jszwedko"]
pr_numbers: [8472]
release: "0.16.0"
hide_on_release_notes: false
badges:
  type: "announcement"
  domains: ["performance"]
---

This release removes some conservative defaults for HTTP-based sink request rate
limiting and concurrency.

Previously, many sinks were defaulting to a limit of 5 requests / second (for
example the `http` sink) with a maximum number of concurrent requests of `5`,
but these limits were rather arbitrary and artificially constrained throughput.
With this release, we've updated the default for most HTTP-based components to
have no rate limiting and a maximum number of concurrent requests of 1024.

To configure a request rate limit or maximum concurrency on a HTTP-based sink,
you can set the `request` parameters like:

```toml
request.concurrency = 5 # limit to 5 in-flight requests
request.rate_limit_num = 10 # limit to 10 requests / second
```

If you haven't already, we recommend trying out our [adaptive concurrency
controller](/blog/adaptive-request-concurrency/) to have Vector
automatically determine the optimal number of in-flight requests to maximize
throughput. You can do this by setting `request.concurrency = "adaptive"`. We
are planning for this to be the default behavior in `v0.17.0`.
