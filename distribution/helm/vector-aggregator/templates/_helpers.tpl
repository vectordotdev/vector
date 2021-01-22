{{/* vim: set filetype=mustache: */}}

{{/*
Internal template to render service ports depending on whether service is a headless service or not. Use
either 'vector-aggregator.servicePorts' or 'vector-aggregator.headlessServicePorts' as entry points.
*/}}
{{- define "vector-aggregator.internalServicePorts" -}}
{{- $headless := index . 0 -}}
{{- $values := index . 1 -}}
{{- if $values.vectorSource.enabled }}
- name: vector
{{- if and $values.vectorSource.nodePort (not $headless) }}
  nodePort: {{ $values.vectorSource.nodePort }}
{{- end }}
  port: {{ $values.vectorSource.listenPort }}
  protocol: TCP
{{- if not $headless }}
  targetPort: {{ $values.vectorSource.listenPort }}
{{- end }}
{{- end }}
{{- range $values.service.ports }}
- port: {{ .port }}
{{- if not $headless }}
  targetPort: {{ .targetPort }}
{{- end }}
{{- if and .nodePort (not $headless) }}
  nodePort: {{ .nodePort }}
{{- end }}
{{- with .name }}
  name: {{.}}
{{- end }}
{{- with .protocol }}
  protocol: {{.}}
{{- end }}
{{- with .appProtocol }}
  appProtocol: {{.}}
{{- end }}
{{- end }}
{{- end -}}

{{/*
Generate effective service ports for normal (non-headless) service definition.
*/}}
{{- define "vector-aggregator.servicePorts" -}}
{{- tuple false .Values | include "vector-aggregator.internalServicePorts" -}}
{{- end -}}

{{/*
Generate effective service ports for headless service definition.
*/}}
{{- define "vector-aggregator.headlessServicePorts" -}}
{{- tuple true .Values | include "vector-aggregator.internalServicePorts" -}}
{{- end }}

{{/*
Determines whether there are any ports present.
*/}}
{{- define "vector-aggregator.servicePortsPresent" -}}
{{- or .Values.vectorSource.enabled (not (empty .Values.service.ports)) }}
{{- end }}
