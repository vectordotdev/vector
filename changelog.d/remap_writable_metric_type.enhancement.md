The `remap` transform can now change a metric's type between `counter` and `gauge` by assigning to `.type`, for example `.type = "counter"`. The metric's value and kind are kept. This is useful when a source can't tell a metric's type, such as Prometheus remote write samples that arrive without type metadata and default to gauges.

Like other disallowed metric changes, assigning any other value to `.type`, or changing the type of a set, distribution, histogram, summary, or sketch metric, is ignored and leaves the metric unchanged.

authors: sghall
