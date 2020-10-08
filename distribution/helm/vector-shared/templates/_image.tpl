{{/* vim: set filetype=mustache: */}}

{{/*
Resolve the actual image tag to use.
*/}}
{{- define "libvector.imageTag" -}}
{{- $localImage := default (dict) .Values.image }}
{{- $global := default (dict) .Values.global }}
{{- $global := default (dict) $global.vector }}
{{- $globalImage := default (dict) $global.image }}
{{- $resolvedImageTag := default $localImage.tag $globalImage.tag }}
{{- if $resolvedImageTag }}
{{- $resolvedImageTag }}
{{- else }}
{{- $resolvedVersion := default $localImage.version $globalImage.version  }}
{{- $resolvedBase := default $localImage.base $globalImage.base }}
{{- $version := default .Chart.AppVersion $resolvedVersion }}
{{- printf "%s-%s" $version $resolvedBase }}
{{- end }}
{{- end }}

{{/*
Resolve the actual image repository to use.
*/}}
{{- define "libvector.imageRepository" -}}
{{- $localImage := default (dict) .Values.image }}
{{- $global := default (dict) .Values.global }}
{{- $global := default (dict) $global.vector }}
{{- $globalImage := default (dict) $global.image }}
{{- default $localImage.repository $globalImage.repository }}
{{- end }}

{{/*
Resolve the full image name to use.
*/}}
{{- define "libvector.image" -}}
{{ include "libvector.imageRepository" . }}:{{ include "libvector.imageTag" . }}
{{- end }}
