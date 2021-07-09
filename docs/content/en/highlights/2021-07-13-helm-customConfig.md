---
date: "2021-07-13"
title: "Introduction of `customConfig` and deprecation notice for TOML based config keys"
description: "Configure Vector directly through your `values.yaml` without having to converting to TOML"
authors: ["spencergilbert"]
pr_numbers: [8079]
release: "0.15.0"
hide_on_release_notes: false
badges:
  type: "deprecation"
  platforms: ["helm"]
  domains: ["config"]
---

...

## Upgrade Guide

We've configured the ConfigMap template to `fail` if the deprecated keys are
enabled at the same time as the new YAML based configurations. The following
values can be used to disable the old deprecated keys and use the new YAML
based configuration with default values.

```yaml title="disable-deprecated.yaml"
globalOptions:
  enabled: false
logSchema:
  enabled: false
vectorApi:
  enabled: false
kubernetesLogsSource:
  enabled: false
vectorSource:
  enabled: false
vectorSink:
  enabled: false
internalMetricsSource:
  enabled: false
hostMetricsSource:
  enabled: false
prometheusSink:
  enabled: false
```

With the deprecated keys disabled, custom Vector configuration can be provided
in raw YAML and is passed through the `tpl` function to allow templating.

Below is an example of values using `customConfig` and templating.

```yaml title="cusomConfig.yaml"
customConfig:
  data_dir: "/custom-data-dir"
  healthchecks:
    enabled: true
    require_healthy: false
  api:
    enabled: true
    address: "127.0.0.1:{{ with index .Values.service.ports 0 }}{{ .port }}{{ end }}"
    playground: false
  sources:
    internal_logs:
      type: internal_logs
    internal_metrics:
      type: internal_metrics
    kubernetes_logs:
      type: kubernetes_logs
      glob_minimum_cooldown_ms: 1000
      ingestion_timestamp_field: ingestion_timestamp
    statsd_metrics:
      type: statsd
      address: "0.0.0.0:{{ with index .Values.service.ports 1 }}{{ .port }}{{ end }}"
      mode: tcp
  transforms:
    sample:
      type: sample
      inputs: ["*_logs"]
      rate: 20
  sinks:
    datadog_logs:
      type: datadog_logs
      inputs: ["sample"]
      compression: gzip
      default_api_key: "${DATADOG_API_KEY}"
      encoding:
        codec: json
    datadog_metrics:
      type: datadog_metrics
      inputs: ["*_metrics"]
      api_key: "${DATADOG_API_KEY}"
service:
  enabled: true
  ports:
  - name: api
    port: 8686
    protocol: TCP
    targetPort: 8686
  - name: statsd
    port: 8000
    protocol: TCP
    targetPort: 8000
# Disable deprecated keys
globalOptions:
  enabled: false
logSchema:
  enabled: false
vectorApi:
  enabled: false
kubernetesLogsSource:
  enabled: false
vectorSource:
  enabled: false
vectorSink:
  enabled: false
internalMetricsSource:
  enabled: false
hostMetricsSource:
  enabled: false
prometheusSink:
  enabled: false
```
