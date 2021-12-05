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
{{- default (include "haproxy.fullname" .) .Values.haproxy.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.haproxy.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
HAProxy labels
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

{{/*
Helper to create HAProxy server-templates to discover Vector endpoints
*/}}
{{- define "haproxy.vectorConfig" -}}
{{- $values := .Values -}}
{{- if $values.vectorSource.enabled }}
frontend vector
  bind :::{{ $values.vectorSource.listenPort }} proto h2
  mode http
  option httplog
  default_backend vector

backend vector
  mode http
  balance roundrobin
  option tcp-check
  server-template srv {{ max 100 (int $values.replicas) }} _vector._tcp.{{ include "libvector.fullname" $ }}-headless.{{ $.Release.Namespace }}.svc.{{ $.Values.global.clusterDomain }} resolvers coredns proto h2 check
{{- end }}
{{ range $item := $values.service.ports }}
frontend {{ $item.name }}
  bind :::{{ $item.port }}
  option httplog
  default_backend {{ $item.name }}

backend {{ $item.name }}
  balance roundrobin
  option tcp-check
  server-template srv {{ max 100 (int $values.replicas) }} _{{ $item.name }}._tcp.{{ include "libvector.fullname" $ }}-headless.{{ $.Release.Namespace }}.svc.{{ $.Values.global.clusterDomain }} resolvers coredns check
{{ end }}
{{- end }}
