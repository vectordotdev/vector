Vector can now emulate a [Prometheus Pushgateway](https://github.com/prometheus/pushgateway) through the new `prometheus_pushgateway` source. Counters and histograms can optionally be aggregated across pushes to support use-cases like cron jobs.

There are some caveats, which are listed [here](https://github.com/Sinjo/vector/blob/0d4fc20091ddae7f3562bfdf07c9095c0c7223e0/src/sources/prometheus/pushgateway.rs#L8-L12).

authors: Sinjo
