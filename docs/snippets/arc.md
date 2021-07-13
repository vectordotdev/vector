#### Adaptive Request Concurrency (ARC)

Adaptive Requst Concurrency is a feature of Vector that does away with static rate limits and automatically optimizes HTTP concurrency limits based on downstream service responses. The underlying mechanism is a feedback loop inspired by TCP congestion control algorithms. See the [announcement blog post][blog_post] for more information.

We strongly recommend enabling this feature as it improves performance and reliability of Vector and the systems it communicates with.

To enable, set the `request.concurrency` option to [`adaptive`](#adaptive):

```toml title="vector.toml"
[sinks.my-sink]
request.concurrency = "adaptive"
```

#### Static rate limits

If Adaptive Request Concurrency is not for you, you can manually set static rate limits with the `request.rate_limit_duration_secs`, `request.rate_limit_num`, and `request.concurrency` options:

```toml title="vector.toml"
[sinks.my-sink]
request.rate_limit_duration_secs = 1
request.rate_limit_num = 10
request.concurrency = 10
```

[blog_post]: /blog/adaptive-request-concurrency
