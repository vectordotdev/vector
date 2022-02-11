# RFC 7709 - 2021-06-29 - Helm: Update Vector configuration pattern

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

Our default configurations are provided directly in the ConfigMap raw, or containing minimal helper templates if needed.
Users can opt-out of our configuration by providing their configuration file under a `customConfig` key which disables
our default configurations for Vector and it's related Kubernetes resources.

The following example for the vector-agent chart ignores backward compatibility with existing keys for simplicity.
The ConfigMap template would be replaced with the following:

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

To make it easier for users to make minor changes to our default configuration we could provide a copy of the
default configuration (commented out). Users would uncomment the block and be able to make any required changes
without having to rewrite the entire configuration.

```yaml
# Specify custom contents for the Vector config
## ref: https://vector.dev/docs/reference/configuration/
## Note a complete and valid configuration is required
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

We will be able to support the existing deprecated fields until the 1.0 release by not rendering the new YAML based
configuration if any deprecated config key is used or enabled, and if a combination of new and old keys are used we
can use a `fail` function to terminate chart rendering early.

## Doc-level Proposal

The only page we currently have with documentation around Helm is on the Kubernetes platform installation page, which
today does not mention how to configure any of the default components.

## Rationale

- Provides the easiest mechanism for users to bring their own configuration and disable our provided default config
- The entire vector configuration will be in a single location for better readability and debuggability
  - We remove a source of bugs by removing any merging of configuration options (between default provided and user provided)
- Reduces the templating logic for the entire configuration file to an `if/else` statement and a one-liner to template the provided configuration (if needed)
- By passing the `customConfig` through a `tpl` function users can more easily generate parts of their configuration from other values (ports, volumes, etc)
- Matches the [Datadog Agent configuration](https://github.com/DataDog/helm-charts/blob/master/charts/datadog/values.yaml#L1023-L1048)
- Easier to support [#7902](https://github.com/vectordotdev/vector/issues/7902) since they can all be toggled off by providing a `customConfig`

## Drawbacks

- Adjusting configuration for individual default components requires replacing the entire configuration
  - This can be mitigated by providing our default or an example configuration commented out in the `values.yaml`
- More user facing changes than the alternatives

## Alternatives

### Do nothing

- Maintains a separate and unique way of configuring specific components we consider "default" that does not align with user provided configuration.
- Maintains extra code to handle the templating of our default configurations, new defaults would require new keys and templating.

### Exclusively use generic `sources`, `transforms`, and `sinks` keys

- Deprecate unique default components and move their configuration into the generic keys we include for user supplied configurations.
- Adding additional configurations, or updating default configurations is easier, as Helm will merge the configuration and replace as needed.
- As we convert from TOML to YAML based configuration files, the need for top level component keys is diminished.
- Users would need to disable components, per component with a `null` value, example for the vector-agent chart:

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

- [ ] Announce plan to deprecate current keys with a highlight in the 0.15.0 release, and mark the values as deprecated in the `values.yaml`
- [ ] If any deprecated key is used or set as enabled, block usage of new configuration and `customConfig` pattern
- [ ] Reverse behavior in upcoming release (0.16.0/0.17.0) to require opt-in to use the deprecated keys, using the new configuration pattern by default
- [ ] Remove the `sources`, `transforms`, `sinks`, and "unique" default keys in future release (1.0 unless maintenance burden is costly)
