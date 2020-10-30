{{/* vim: set filetype=mustache: */}}

{{/*
Common container env variables.
*/}}
{{- define "libvector.globalEnv" -}}
{{- $global := default (dict) .Values.global }}
{{- $global := default (dict) $global.vector }}
{{- range $key, $value := $global.commonEnvKV }}
- name: {{ $key | quote }}
  value: {{ $value | quote }}
{{- end }}
{{- end }}
