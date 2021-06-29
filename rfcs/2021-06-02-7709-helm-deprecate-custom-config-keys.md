# RFC 7709 - 2021-06-02 - Helm: Deprecate "custom" configuration keys

This RFC proposes an update for our current configuration to fully embrace our support of YAML configuration files.

## Scope

This RFC will cover the component configuration keys and the default `sinks` and `sources` defined under unique keys in 
our Helm charts today. We provide defaults configurations for the following components:

vector-agent:

- `kubernetes_logs` source
- `internal_metrics` source
- `host_metrics` source
- `vector` sink
- `prometheus_exporter` sink

vector-aggregator:

- `vector` source
- `internal_metrics` source
- `prometheus_exporter` sink

## Motivation

Our current Helm chart for configuring Vector in Kubernetes provides shortcuts for configuring common
`sources` and `sinks`, while also allowing users to specify their configuration in the more traditional "direct"
fashion, similar to configuring Vector when running on a virtual machine. These two approaches being
used simultaneously can lead to issues, and general confusion, about what the correct way to configure Vector is.

## Internal Proposal

...

Ignoring backward compatibility with existing keys, the ConfigMap template would be replaced with the following:

```yaml
{{- if (empty .Values.existingConfigMap) -}}
apiVersion: v1
kind: ConfigMap
metadata:
  name: {{ include "libvector.configMapName" . }}
  labels:
    {{- include "libvector.labels" . | nindent 4 }}
data:
  {{- if .Values.customConfig }}
  vector.yaml: |
{{ tpl (toYaml .Values.customConfig) . | indent 4 }}
  {{- else }}
  vector.yaml: |
    # Docs: https://vector.dev/docs/
    data_dir: "/vector-data-dir"
    log_schema:
      host_key: host
      message_key: message
      source_type_key: source_type
      timestamp_key: timestamp
    sources:
      host_metrics:
        type: host_metrics
        filesystem:
          devices:
            excludes: ["binfmt_misc"]
          filesystems:
            excludes: ["binfmt_misc"]
          mountpoints:
            excludes: ["*/proc/sys/fs/binfmt_misc"]
      internal_metrics:
        type: internal_metrics
      kubernetes_logs:
        type: kubernetes_logs
    sinks:
      prometheus_sink:
        type: prometheus_exporter
        inputs: ["host_metrics", "internal_metrics"]
        address: 0.0.0.0:9090
  {{- end }}
{{- end }}
```

To make it easier for users to make minor changes to our default configuration we would provide a copy of the
default configuration (commented out). Users would uncomment the block and be able to make any required changes
without having to rewrite the entire configuration.

```yaml
customConfig: {}
  #  data_dir: "/vector-data-dir"
  #  log_schema:
  #    host_key: host
  #    message_key: message
  #    source_type_key: source_type
  #    timestamp_key: timestamp
  #  sources:
  #    host_metrics:
  #      type: host_metrics
  #      filesystem:
  #        devices:
  #          excludes: ["binfmt_misc"]
  #        filesystems:
  #          excludes: ["binfmt_misc"]
  #        mountpoints:
  #          excludes: ["*/proc/sys/fs/binfmt_misc"]
  #    internal_metrics:
  #      type: internal_metrics
  #    kubernetes_logs:
  #      type: kubernetes_logs
  #  sinks:
  #    prometheus_sink:
  #      type: prometheus_exporter
  #      inputs: ["host_metrics", "internal_metrics"]
  #      address: 0.0.0.0:9090
```

## Doc-level Proposal

The only page we currently have with documentation around Helm is on the Kubernetes platform installation page, which today does not mention how to configure
any of the default components.

## Rationale

- Reduce template logic
- Configuration files are more obvious/readable/apparent
- More easily support #7902
- More easily configure related Service and port options
- Matches [Datadog Agent configuration](https://github.com/DataDog/helm-charts/blob/master/charts/datadog/values.yaml#L1023-L1048)
- ...

## Drawbacks

- Adjusting configuration for individual default components requires replacing the entire configuration
- More user facing changes than the alternatives
- ...

## Alternatives

### Do nothing

- Maintains a separate and unique way of configuring specific components we consider "default" that does not align with user provided configuration.
- Maintains extra code to handle the templating of our default configurations, new defaults would require new keys and templating.

### Exclusively use generic `sources`, `transforms`, and `sinks` keys

- Deprecate unique default components and move their configuration into the generic keys we include for user supplied configurations.
- As we also convert from TOML to YAML based configuration files, we continue to maintain an unnecessary set of keys: `sources`, `transforms`, and `sinks`. We can simply `tpl` or `toYaml` YAML config from the `values.yaml` into our Vector configuration file(s).
- Users would need to disable components, per component with a `null` value, example:

```yaml
sources:
  kubernetes_logs: null
  internal_metrics: null
  host_metrics: null
sinks:
  vector_sink: null
  prometheus_sink: null
```

## Plan Of Attack

- [ ] Announce plan to deprecate current keys with a highlight in the 0.15.0 release
- [ ] TODO: Phase in ...
- [ ] TODO: Phase out ...
- [ ] Remove the `sources`, `transforms`, `sinks`, and "unique" default keys in future release
