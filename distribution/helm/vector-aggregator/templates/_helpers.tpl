{{/* vim: set filetype=mustache: */}}

{{/*
Resolve effective service ports to use.
*/}}
{{- define "vector-aggregator.servicePorts" -}}
{{- with .Values.service.ports }}
{{ toYaml . }}
{{- end }}
{{- end }}
