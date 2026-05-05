The `aggregate` transform now supports event-time aggregation. This avoids collapsing distinct
samples in sinks (such as Datadog Metrics) that overwrite earlier values for an identical
timestamp.

authors: kaarolch
