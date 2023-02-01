---
date: "2021-10-06"
title: "HTTP-based sinks now use adaptive concurrency by default"
description: ""
authors: ["jszwedko"]
pr_numbers: []
release: "0.17.0"
hide_on_release_notes: false
badges:
  type: "announcement"
---

Originally released as an opt-in feature in [v0.11.0][0.11.0], Vector's adaptive
concurrency has been promoted to the default behavior for all HTTP-based sinks.
This was following months of testing and feedback from users that had opted into
adaptive concurrency. We expect users to see an overall improvement in the
throughput of Vector along with Vector automatically backing off in the face of
increased pressure downstream, to avoid overwhelming the sink destination.

This feature was previously able to be opted into with the following
configuration:

```toml
request.concurrency = "adaptive"
```

This is the new default for HTTP-based sinks. As mentioned in the [announcement
blog post][0.11.0] the adaptive concurrency controller will automatically
respond to back-pressure from the destination of the sink (in the form of
increased response times or explicit HTTP 429 response codes, among others).

We welcome any feedback from users in our [Discord server][chat]!

If you would like to instead use a fixed concurrency as was previously the
default, you can set a static value like:


```toml
request.concurrency = 5
```

This will tell Vector to limit to 5 concurrent requests.

[chat]: https://chat.vector.dev
[0.11.0]: https://vector.dev/blog/adaptive-request-concurrency/
