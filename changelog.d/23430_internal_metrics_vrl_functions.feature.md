Functions to access internal Vector metrics are now available for VRL: `get_vector_metric`, `find_vector_metrics` and `aggregate_vector_metrics`. They work with a snapshot of the metrics and the interval the snapshot is taken in can be controlled with `metrics_storage_refresh_period` global option. Aggregation supports `max`, `avg`, `min` and `max` functions.

authors: esensar Quad9DNS
