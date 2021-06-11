{{/* vim: set filetype=mustache: */}}

{{/*
Create a default fully qualified app name.
We truncate at 63 chars because some Kubernetes name fields are limited to this
(by the DNS naming spec).
If release name is "vector", or matches the chart name exactly, the chart name
is used without the release name, omitting the extra "vector-" or
"[chart-name]-" prefix.
*/}}
{{- define "haproxy.fullname" -}}
{{- printf "%s-haproxy" (include "libvector.fullname" .) | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create the name of the `ServiceAccount` to use.
*/}}
{{- define "haproxy.serviceAccountName" -}}
{{- if .Values.haproxy.serviceAccount.create }}
{{- default (include "libvector.fullname" .) .Values.haproxy.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.haproxy.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Compact labels
*/}}
{{- define "haproxy.labels" -}}
{{ include "libvector.labels" . }}
app.kubernetes.io/component: load-balancer
{{- end }}

{{/*
Selector labels
*/}}
{{- define "haproxy.selectorLabels" -}}
{{ include "libvector.selectorLabels" . }}
app.kubernetes.io/component: load-balancer
{{- end }}
