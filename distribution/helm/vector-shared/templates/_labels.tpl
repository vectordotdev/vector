{{/* vim: set filetype=mustache: */}}

{{/*
Common labels.
*/}}
{{- define "libvector.labels" -}}
helm.sh/chart: {{ include "libvector.chart" . }}
{{ include "libvector.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: {{ include "libvector.name" . }}
{{- if .Values.global.commonLabels }}
{{ toYaml .Values.global.commonLabels }}
{{- end }}
{{- end }}

{{/*
Selector labels.
*/}}
{{- define "libvector.selectorLabels" -}}
app.kubernetes.io/name: {{ include "libvector.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}
