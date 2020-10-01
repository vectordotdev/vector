{{/* vim: set filetype=mustache: */}}
{{/*
Resolve the actual image tag to use.
*/}}
{{- define "libvector.imageTag" -}}
{{- if .Values.image.tag }}
{{- .Values.image.tag }}
{{- else }}
{{- $version := default .Chart.AppVersion .Values.image.version }}
{{- printf "%s-%s" $version .Values.image.base }}
{{- end }}
{{- end }}
