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
