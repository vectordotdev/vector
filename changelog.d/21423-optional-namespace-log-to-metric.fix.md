When using the `all_metrics: true` flag in `log_to_metric` transform, the `namespace` field is now optional and no longer required. If the `namespace` field is not provided,
the produced metric will not have a namespace at all.
