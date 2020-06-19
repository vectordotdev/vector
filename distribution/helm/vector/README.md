# Vector Chart

[Vector](https://vector.dev/) - a lightweight and ultra-fast tool for building observability pipelines.

- [Chart Details](#chart-details)
- [Installing the chart](#installing-the-chart)
- [Configuration](#configuration)
- [Examples](#examples)

## Chart Details

This chart will do the following:

- Create a `ConfigMap` with Vector configuration;
- Install a `DaemonSet` that provisions Vector;
- Deploy service and service monitor for integration with prometheus-operator (if enabled);

## Installing the Chart

To install the chart with the release name `vector` and namespace `vector`:

### Helm 2

```bash
$ cd distribution/helm/vector
$ helm install --namespace vector --name vector .
```

### Helm 3

```bash
$ cd distribution/helm/vector
$ helm install --create-namespace --namespace vector vector .
```

## Configuration

The following table lists the configurable parameters of the Vector chart and the default values.

| Parameter                     | Description                        | Default                 |
| ----------------------------- | ---------------------------------- | ----------------------- |
| **General**                   |
| `image.repository`            | Image                              | `timberio/vector`       |
| `image.tag`                   | Image tag                          | `0.8.X-debian`          |
| `image.pullPolicy`            | Image pull policy                  | `Always`                |
| `image.pullSecrets`           | Image pull secrets                 | `nil`                   |
| `updateStrategy`              | DaemonSet update strategy          | `RollingUpdate`         |
| `rbac.enabled`                      | Specifies whether RBAC should be enabled.                                                    | `true`    |
| `rbac.apiVersion`                   | Overrides K8S API version for RBAC (by default it's determined using `Capabilities`)         | `nil`     |
| `rbac.serviceAccount.name`          | Overrides service acount name, if not provided and RBAC is disabled, `default` will be used. | `nil`     |
| `rbac.serviceAccount.annotations`   | Additional annotation for the created service account.                                       | `{}`      |
| `rbac.psp.enabled`                  | Specifies whether a PodSecurityPolicy should be created.                                     | `false`   |
| `rbac.apiVersion`                   | Overrides K8S API version for PSP (by default it's determined using `Capabilities`).         | `nil`     |
| `env`                    | A list of environment variables to be used for the DaemonSet.  | `[]` |
| `resources`              | Pod resource requests & limits.                                | `{}` |
| `tolerations`            | Optional DaemonSet tolerations.                                | `[]` |
| `nodeSelector`           | Node labels for pod assignment.                                | `{}` |
| `affinity`               | Expressions for affinity.                                      | `{}` |
| `extraVolumes`           | Extra volumes to be assigned to pods.                          | `{}` |
| `extraVolumeMounts`      | Extra volume mounts for the vector container.                  | `{}` |
| **ConfigMap**            |
| `existingConfigMap`           | Name of the existing ConfigMap to be used for Vector configuration. | `nil` |
| `globalOptions.dataDir`       | Vector's data directory.                | `/vector-data-dir` |
| `logSchema.hostKey`           | The key used to hold the log host.                     | `host`            |
| `logSchema.messageKey`        | The key used to hold the log message.                  | `message`         |
| `logSchema.sourceTypeKey`     | The key used to hold the log source type.              | `source_type`     |
| `logSchema.timestampKey`      | The key used to represent when the log was generated.  | `timestamp`       |
| `sources.kubernetesLogs.enabled`                | Enables Kubernetes Logs source.                 | `true`       |
| `sources.kubernetesLogs.sourceId`               | Kubernetes source ID.                           | `kubernetes` |
| `sources.kubernetesLogs.rawConfig`              | Raw config to be used for the source.           | `nil`        |
| `sources.additionalSources`                 | An object of additional sources. Key will be used as source ID. | `{}`         |
| `sources.additionalSources.type`            | Source type.                                                    |              |
| `sources.additionalSources.rawConfig`       | Raw config to be used for the additional source.                | `nil`        |
| `transforms`                 | An object of transforms. Key will be used as transform ID. | `{}`  |
| `transforms.type`            | Transform type.                                            |       |
| `transforms.inputs`          | A list of transform data sources.                          |       |
| `transforms.rawConfig`       | Raw config to be used for the transform.                   | `nil` |
| `sinks`                      | An object of sinks. Entry key will be used as sink ID.     | `{}`  |
| `sinks.type`                 | Sink type.                                                 |       |
| `sinks.inputs`               | A list of sink data sources.                               |       |
| `sinks.rawConfig`            | Raw config to be used for the sink.                        | `nil` |


## Examples

Here are some snippets for Vector configuration.

### Overriding Kubernetes source namespaces

Whitelisted namespaces (or other options) can be set for the `kubernetes` source,
just by overriding `sources.kubernetes` options.

```yaml
sources:
  kubernetes:
    includeNamespaces:
      - default
```

### Using transforms

```yaml
transforms:
  envTagging:
    type: add_fields
    inputs:
      - kubernetes
    rawConfig: |
      [transforms.envTagging.fields]
        env = "dev"

  jsonParser:
    type: json_parser
    inputs:
      - envTagging
    rawConfig: |
      field = "message"
      drop_field = true
      drop_invalid = false
```

### Using Splunk sink

In this example the Splunk HEC sink uses token from env variable (referenced from K8S secret).

```yaml
env:
  - name: SPLUNK_TOKEN
    valueFrom:
      secretKeyRef:
        name: vector-credentials
        key: SPLUNK_TOKEN

sinks:
  splunk:
    type: splunk_hec
    inputs:
      - jsonParser
    rawConfig: |
      host = "https://splunk-endpoint.net"
      token = "${SPLUNK_TOKEN}"
      encoding = "json"
      healthcheck = true
      indexed_fields = ["env", "pod_name", "pod_namespace", "container_name"]

```
