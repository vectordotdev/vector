Added a new optional global option `expire_metrics_per_metric_set`, enabling configuration of metrics expiration, similar to `expire_metrics_secs`, but enables defining different values per metric set, defined with a name and/or set of labels. `expire_metrics_secs` is used as a global default for sets not matched by this.

authors: esensar Quad9DNS
