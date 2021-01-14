{{/* vim: set filetype=mustache: */}}

{{/*
Resolve effective service ports to use.
*/}}
{{- define "vector-aggregator.servicePorts" -}}
ports:
{{- if .Values.vectorSource.enabled }}
- name: vector
{{- if and .Values.vectorSource.nodePort (eq "NodePort" .Values.service.type) }}
  nodePort: {{ .Values.vectorSource.nodePort }}
{{- end }}
  port: {{ .Values.vectorSource.listenPort }}
  protocol: TCP
  targetPort: {{ .Values.vectorSource.listenPort }}
{{- end }}
{{- with .Values.service.ports }}
{{ toYaml . }}
{{- end }}
{{- end }}

{{/*
Generate effective service ports omitting the 'nodePort' for headless definition.
*/}}
{{- define "vector-aggregator.headlessServicePorts" -}}
{{- $ports := include "vector-aggregator.servicePorts" . | fromYaml -}}
{{- $headlessPorts := list -}}
{{- range $port := $ports.ports -}}
{{- $headlessPorts = append $headlessPorts (omit $port "nodePort") -}}
{{- end -}}
ports:
{{ $headlessPorts | toYaml | indent 2 }}
{{- end }}

{{/*
Determines whether there are any ports present.
*/}}
{{- define "vector-aggregator.servicePortsPresent" -}}
{{- or .Values.vectorSource.enabled (not (empty .Values.service.ports)) }}
{{- end }}
