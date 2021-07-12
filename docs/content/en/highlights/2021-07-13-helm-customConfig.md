---
date: "2021-07-13"
title: "Introduction of `customConfig` Helm option and deprecation notice for TOML based config keys"
short: "Introduction of `customConfig` in Helm charts"
description: "Configure Vector directly through your Helm `values.yaml` without having to converting to TOML"
authors: ["spencergilbert"]
pr_numbers: [8079]
release: "0.15.0"
hide_on_release_notes: false
badges:
  type: "deprecation"
  platforms: ["helm"]
  domains: ["config"]
---

With the release of Vector 0.15.0, we have introduced a new method of configuring Vector through Helm.
This new method uses YAML configuration files, and in a coming release will become the default configuration
method.

**You do not need to upgrade immediately. The deprecated keys will not be removed before Vector hits 1.0**

## Upgrade Guide

### Breaking: updated container args

The argument passed to Vector has been changed to `--config-dir` which will load any TOML, YAML, or JSON
files added to the "/etc/vector" directory.

If you have been including files that should not be loaded by Vector as configuration you should move them
into a separate directory or into a sub-directory of "etc/vector".

### globalOptions and logSchema

Any [global options](/docs/reference/configuration/global-options/) can be moved directly
into the `customConfig` key and converted into snake-case.

```diff title="values.yaml"
   globalOptions:
+    enabled: false
     dataDir: "/vector-data-dir"
   logSchema:
+    enabled: false
     hostKey: "host"
     messageKey: "message"
     sourceTypeKey: "source_type"
     timestampKey: "timestamp"
   ...
+  customConfig:
+    data_dir: "/vector-data-dir"
+    log_schema:
+      host_key: host
+      message_key: message
+      source_type_key: source_type
+      timestamp_key: timestamp
+  ...
```

### vectorApi

[Vector API](/docs/reference/api/) options can be moved as is under a `customConfig.api` key.
The `extraContainerPorts` or `service` key should be used to expose the port configured in `customConfig`.

```diff title="values.yaml"
   vectorApi:
+    enabled: false
     address: "0.0.0.0:8686"
     playground: true
   ...
+  customConfig:
+    ...
+    api:
+      enabled: true
+      address: 0.0.0.0:8686
+      playground: true
+    ...
```

### kubernetesLogsSource

The vector-agent chart will continue to mount the hostPaths required to access Pod logs.

```diff title="values.yaml"
   kubernetesLogsSource:
+    enabled: false
     sourceId: kubernetes_logs
     config: {}
     rawConfig: null
   ...
+  customConfig:
+    ...
+    sources:
+      ...
+      kubernetes_logs:
+        type: kubernetes_logs
+    ...
```

### vectorSource

The `extraContainerPorts` or `service` key should be used to expose the port configured in `customConfig`.

```diff title="values.yaml"
   vectorSource:
+    enabled: false
     sourceId: vector
     listenAddress: "0.0.0.0"
     listenPort: "9000"
     config: {}
     nodePort: null
     rawConfig: null
   ...
+  customConfig:
+    ...
+    sources:
+      ...
+      vector:
+        type: vector
+        address: 0.0.0.0:9000
+    ...
```

### vectorSink

```diff title="values.yaml"
   vectorSink:
+    enabled: false
     sinkId: vector_sink
     inputs: ["kubernetes_logs"]
     host: vector
     port: "9000"
     config: {}
     rawConfig: null
   ...
+  customConfig:
+    ...
+    sinks:
+      ...
+      vector_sink:
+        type: vector
+        inputs: ["kubernetes_logs"]
+        address: vector:9000
+    ...
```

### internalMetricsSource

```diff title="values.yaml"
   internalMetricsSource:
+    enabled: false
     sourceId: internal_metrics
     config: {}
     rawConfig: null
   ...
+  customConfig:
+    ...
+    sources:
+      ...
+      internal_metrics:
+        type: internal_metrics
+    ...
```

### hostMetricsSource

Users should set the `PROCFS_ROOT` and `SYSFS_ROOT` environment variables, as well as mounting the
required hostPaths.

```diff title="values.yaml"
   hostMetricsSource:
+    enabled: false
     sourceId: host_metrics
     config:
       filesystem:
         devices:
           excludes: [binfmt_misc]
         filesystems:
           excludes: [binfmt_misc]
         mountpoints:
           excludes: ["*/proc/sys/fs/binfmt_misc"]
     rawConfig: null
   ...
+  customConfig:
+    ...
+    sources:
+      ...
+      host_metrics:
+        type: host_metrics
+        filesystem:
+          devices:
+            excludes: ["binfmt_misc"]
+          filesystems:
+            excludes: ["binfmt_misc"]
+          mountPoints:
+            excludes: ["*/proc/sys/fs/binfmt_misc"]
+  env:
+    - name: PROCFS_ROOT
+      value: /host/proc
+    - name: SYSFS_ROOT
+      value: /host/sys
+  extraVolumeMounts:
+    - name: procfs
+      mountPath: /host/proc
+      readOnly: true
+    - name: sysfs
+      mountPath: /host/sys
+      readOnly: true
+  extraVolumes:
+    - name: procfs
+      hostPath:
+        path: /proc
+    - name: sysfs
+      hostPath:
+        path: /sys
+    ...
```

### prometheusSink

The `extraContainerPorts` or `service` key should be used to expose the port configured in `customConfig`.

The `prometheusSink.podMonitor` key has been moved to a top level key and can be accessed directly at
`podMonitor`. The `addPodAnnotations` option has been removed in favor setting the required annotations
with the `podAnnotations` key.

```diff title="values.yaml"
   prometheusSink:
+    enabled: false
     sinkId: prometheus_sink
     inputs: []
     excludeInternalMetrics: false
     listenAddress: "0.0.0.0"
     listenPort: "9090"
     config: {}
     rawConfig: null
   ...
+  customConfig:
+    ...
+    sinks:
+      ...
+      prometheus_sink:
+        type: prometheus_exporter
+        inputs: ["host_metrics", "internal_metrics"]
+        address: 0.0.0.0:9090
+    ...
```

## Using `customConfig`

We've configured the `ConfigMap` template to fail if the deprecated keys are enabled at the same time
as the new YAML based configurations. The following values can be used to disable the old deprecated
keys and use the new YAML based configuration with default values.

With the deprecated keys disabled, a custom Vector configuration can be provided in raw YAML and is passed
through a `tpl` function to allow for the evaluation of Helm templates contained within. Below is an example
of values using `customConfig` and templating.

```yaml title="cusomConfig.yaml"
customConfig:
  data_dir: "/custom-data-dir"
  healthchecks:
    enabled: true
    require_healthy: true
  api:
    enabled: true
    address: "0.0.0.0:{{ with index .Values.service.ports 0 }}{{ .port }}{{ end }}"
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
