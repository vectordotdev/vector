{{/* vim: set filetype=mustache: */}}

{{/*
Common Vector configuration partial containing built-in metrics pipeline.
Internal metrics are common, so we share and reuse the definition.
*/}}
{{- define "libvector.metricsConfigPartial" -}}
{{- with .Values.internalMetricsSource }}
{{- if .enabled }}
# Emit internal Vector metrics.
[sources.{{ .sourceId }}]
  type = "internal_metrics"

  {{- with .rawConfig }}
  {{- . | nindent 6 }}
  {{- end }}
{{- end }}
{{- end }}

{{- with .Values.prometheusSink }}
{{- if .enabled }}
{{- $inputs := .inputs }}
{{- if and $.Values.internalMetricsSource.enabled (not .excludeInternalMetrics) -}}
{{- $inputs = prepend $inputs $.Values.internalMetricsSource.sourceId }}
{{- end }}
# Expose metrics for scraping in the Prometheus format.
[sinks.{{ .sinkId }}]
  type = "prometheus"
  inputs = {{ $inputs | toJson }}
  address = "{{ .listenAddress }}:{{ .listenPort }}"

  {{- with .rawConfig }}
  {{- . | nindent 6 }}
  {{- end }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Common Vector container ports used by the built-in metrics pipeline.
Internal metrics are common, so we share and reuse the definition.
*/}}
{{- define "libvector.metricsContainerPorts" -}}
{{- with .Values.prometheusSink }}
{{- if .enabled -}}
- name: metrics
  containerPort: {{ .listenPort }}
  protocol: TCP
{{- end }}
{{- end }}
{{- end }}

{{/*
Common Vector Pod annotations to expose the built-in metrics pipeline for
Prometheus scraping.
Internal metrics are common, so we share and reuse the definition.
*/}}
{{- define "libvector.metricsPrometheusPodAnnotations" -}}
{{- with .Values.prometheusSink }}
{{- if and .enabled .addPodAnnotations -}}
prometheus.io/scrape: "true"
prometheus.io/port: "{{ .listenPort }}"
{{- end }}
{{- end }}
{{- end }}

{{/*
Common Vector `PodMonitor` to expose the built-in metrics pipeline for
`prometheus-operator`-powered scraping.
Internal metrics are common, so we share and reuse the definition.
*/}}
{{- define "libvector.metricsPodMonitor" -}}
{{- if and .Values.prometheusSink.enabled .Values.prometheusSink.podMonitor.enabled -}}
apiVersion: monitoring.coreos.com/v1
kind: PodMonitor
metadata:
  name: {{ include "libvector.fullname" . }}
  labels:
    {{- include "libvector.labels" . | nindent 4 }}
spec:
  jobLabel: app.kubernetes.io/name

  selector:
    matchLabels:
      {{- include "libvector.selectorLabels" . | nindent 6 }}

  namespaceSelector:
    matchNames:
      - "{{ .Release.Namespace }}"

  podMetricsEndpoints:
    - port: metrics
      path: /metrics
      relabelings:
        - action: labeldrop
          regex: __meta_kubernetes_pod_label_skaffold_dev.*
        - action: labeldrop
          regex: __meta_kubernetes_pod_label_pod_template_hash.*
        - action: labelmap
          regex: __meta_kubernetes_pod_label_(.+)
        {{- with .Values.prometheusSink.podMonitor.extraRelabelings }}
        {{- toYaml . | nindent 8 }}
        {{- end }}
{{- end }}
{{- end }}
