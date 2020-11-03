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
