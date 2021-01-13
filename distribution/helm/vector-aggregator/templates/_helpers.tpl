{{/* vim: set filetype=mustache: */}}

{{/*
Resolve effective service ports to use.
*/}}
{{- define "vector-aggregator.servicePorts" -}}
{{- if .Values.vectorSource.enabled }}
- name: vector
{{- with .Values.vectorSource.nodePort }}
  nodePort: {{ . }}
{{- end }}
  port: {{ .Values.vectorSource.listenPort }}
  protocol: TCP
  targetPort: {{ .Values.vectorSource.listenPort }}
{{- end }}
{{- with .Values.service.ports }}
{{ toYaml . }}
{{- end }}
{{- end }}

{{/*
Determines whether there are any ports present.
*/}}
{{- define "vector-aggregator.servicePortsPresent" -}}
{{- or .Values.vectorSource.enabled (not (empty .Values.service.ports)) }}
{{- end }}
