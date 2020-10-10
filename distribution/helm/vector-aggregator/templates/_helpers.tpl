{{/* vim: set filetype=mustache: */}}

{{/*
Resolve effective service ports to use.
*/}}
{{- define "vector-aggregator.servicePorts" -}}
{{- if .Values.vectorSource.enabled }}
- name: vector
  port: {{ .Values.vectorSource.listenPort }}
  protocol: TCP
  targetPort: {{ .Values.vectorSource.listenPort }}
{{- end }}
{{- with .Values.service.ports }}
{{ toYaml . }}
{{- end }}
{{- end }}
