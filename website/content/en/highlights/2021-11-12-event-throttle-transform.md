---
date: "2021-11-18"
title: "Event `throttle` transform released"
description: "A guide to using the new `throttle` transform"
authors: ["barieom"]
pr_numbers: [9378]
release: "0.18.0"
hide_on_release_notes: false
badges:
  type: new feature
---

We've released a new `throttle` transform that provides a user the ability to
throttle the throughput of specific event streams.

Large spikes in the volume of observability data can have undesirable impacts
including increased costs for sink destinations that price based on volume or
accidentally overwhelming internally deployed services like Loki.

To protect yourself from these effects, we've added a new [`throttle`][throttle]
transform to Vector.  The [`throttle`][throttle] transform enables you to rate
limit specific subsets of your event stream to limit load on downstream services
or enforce quotas on specific services (identified by fields on the log event).
You can also exclude events based on a [VRL condition] to avoid dropping
critical logs.

To ensure that each bucket's throughput averages out to the `threshold` per
`window`, rate limiting spreads load across the configured `window` (configured
by `window_secs`). The rate limiter will allow up to threshold number of events
through and drop any further events for that particular bucket when the rate
limiter is at capacity.

A rate limiter is created with a maximum number of cells equal to the
`threshold`, with cells replenishing at a rate of `window` divided by
`threshold`. For example, a `window` of 60 with a `threshold` of 10 replenishes
a cell every 6 seconds and allows a burst of up to 10 events.

See the following simple example.

Given these incoming log events:

```json
[
  {"host":"host-1.hostname.com","message":"First message","timestamp":"2020-10-07T12:33:21.223543Z"},
  {"host":"host-1.hostname.com","message":"Second message","timestamp":"2020-10-07T12:33:21.223543Z"}
]
```

...and this configuration...

```toml
[transforms.my_transform_id]
type = "throttle"
inputs = [ "my-source-or-transform-id" ]
threshold = 1
window_secs = 60
```

...only one event will be allowed through:

```json
{"host":"host-1.hostname.com","message":"First message","timestamp":"2020-10-07T12:33:21.223543Z"}
```

Due to the threshold of `1` event per 60 seconds.

If you any feedback, let us know on our [Discord chat] or [Twitter].

[throttle]: /docs/reference/configuration/transforms/throttle/
[VRL condition]: /docs/reference/vrl/#example-filtering-events
[Discord chat]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev
