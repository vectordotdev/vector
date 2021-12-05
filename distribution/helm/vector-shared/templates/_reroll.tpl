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
Values checksum annotation.
*/}}
{{- define "libvector.valuesChecksumAnnotation" -}}
{{- $global := default (dict) .Values.global }}
{{- $global := default (dict) $global.vector }}
{{- $enabled := .Values.podValuesChecksumAnnotation }}
{{- if hasKey $global "podValuesChecksumAnnotation" }}
{{- $enabled = $global.podValuesChecksumAnnotation }}
{{- end }}
{{- if $enabled }}
checksum/values: {{ toJson .Values | sha256sum }}
{{- end }}
{{- end }}

{{/*
All reroll annotations.
*/}}
{{- define "libvector.rerollAnnotations" -}}
{{- include "libvector.valuesChecksumAnnotation" . }}
{{- include "libvector.rollmeAnnotation" . }}
{{- end }}
