---
date: "2021-11-16"
title: "Event trottle transform released"
description: "A guide that addresses the new event throttle transform"
authors: ["barieom"]
pr_numbers: []
release: "0.18.0"
hide_on_release_notes: false
badges:
  type: breaking change
---

## Event trottle transform released


We've released a new transform that provides a user the ability to control the throughput of specific event streams.

Large spikes in data volume can frequently overwhelm a service, which is especially common in log data. Previously, users lacked the necessary tooling to control throughput, such as setting quota for users and user groups utilizing Vector, which not only can cause spike in costs, but also negatively impact downstream services due to the increased load. 

The [`throttle`](https://master.vector.dev/docs/reference/configuration/transforms/throttle/) transform enables you to rate limit specific subsets of your event stream to limit load on downstream services or enforce quotas on users. You can utilize the `throttle` transform to enforce rate limits on number of events and exclude events based on a VRL condition to avoid dropping critical logs. 

To ensure that each bucket's throughput averages out to the `threshold` per `window`, rate limiting spreads load across the configured `window`. The rate limiter will allow up to threshold number of events through and drop any further events for that particular bucket when the rate limiter is at capacity. 

A rate limiter is created with a maximum number of cells equal to the `threshold`, with cells replenishing at a rate of `window` divided by `threshold`. To paint a better example, a `window` of 60 with a `threshold` of 10 replenishes a cell every 6 seconds and allows a burst of up to 10 events. 

For example,

Given this event...
```
[{"log":{"host":"host-1.hostname.com","message":"First message","timestamp":"2020-10-07T12:33:21.223543Z"}},{"log":{"host":"host-1.hostname.com","message":"Second message","timestamp":"2020-10-07T12:33:21.223543Z"}}]
```

...and this configuration...
```toml
[transforms.my_transform_id]
type = "throttle"
inputs = [ "my-source-or-transform-id" ]
threshold = 1
window = 60
```

...this Vector event is produced:
```
[{"log":{"host":"host-1.hostname.com","message":"First message","timestamp":"2020-10-07T12:33:21.223543Z"}}]
```

Let us know if you any feedback [here](https://master.vector.dev/docs/reference/configuration/transforms/throttle/).