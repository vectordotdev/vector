{{/* vim: set filetype=mustache: */}}

{{/*
Common partial to defining an individual service port. The template will render a port entry for
a Service resource according to whether the service is headless or not.
*/}}
{{- define "libvector.servicePort" -}}
{{- $headless := index . 0 -}}
{{- $values := index . 1 -}}
- port: {{ $values.port }}
{{- if not $headless }}
  targetPort: {{ $values.targetPort }}
{{- end }}
{{- if and $values.nodePort (not $headless) }}
  nodePort: {{ $values.nodePort }}
{{- end }}
{{- with $values.name }}
  name: {{.}}
{{- end }}
{{- with $values.protocol }}
  protocol: {{.}}
{{- end }}
{{- with $values.appProtocol }}
  appProtocol: {{.}}
{{- end }}
{{- end -}}
