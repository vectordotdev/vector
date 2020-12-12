{{/* vim: set filetype=mustache: */}}

{{/*
Rollme annotation.
*/}}
{{- define "libvector.rollmeAnnotation" -}}
{{- $global := default (dict) .Values.global }}
{{- $global := default (dict) $global.vector }}
{{- $enabled := .Values.podRollmeAnnotation }}
{{- if hasKey $global "podRollmeAnnotation" }}
{{- $enabled = $global.podRollmeAnnotation }}
{{- end }}
{{- if $enabled }}
rollme: {{ randAlphaNum 5 | quote }}
{{- end }}
{{- end }}

{{/*
`ConfigMap` template checksum annotation.
*/}}
{{- define "libvector.configTemplateChecksumAnnotation" -}}
{{- if not .Values.externalConfigMap }}
checksum/config: {{ include (print $.Template.BasePath "/configmap.yaml") . | sha256sum }}
{{- end }}
{{- end }}
