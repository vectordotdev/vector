{{/*
Internal template to render service ports depending on whether service is a headless service or not. Use
either 'vector-aggregator.servicePorts' or 'vector-aggregator.headlessServicePorts' as entry points.
*/}}
{{- define "vector-aggregator.internalServicePorts" -}}
{{- $headless := index . 0 -}}
{{- $values := index . 1 -}}
{{- if and $values.vectorSource.enabled (not $values.customConfig) }}
{{- $servicePort := dict -}}
{{- $_ := set $servicePort "name" "vector" -}}
{{- $_ := set $servicePort "port" $values.vectorSource.listenPort -}}
{{- $_ := set $servicePort "nodePort" $values.vectorSource.nodePort -}}
{{- $_ := set $servicePort "protocol" "TCP" -}}
{{- $_ := set $servicePort "targetPort" $values.vectorSource.listenPort -}}
{{ tuple $headless $servicePort | include "libvector.servicePort" }}
{{- end }}
{{- range $values.service.ports }}
{{ tuple $headless . | include "libvector.servicePort" }}
{{- end }}
{{- end -}}

{{/*
Generate effective service ports for normal (non-headless) service definition.
*/}}
{{- define "vector-aggregator.servicePorts" -}}
{{- tuple false .Values | include "vector-aggregator.internalServicePorts" -}}
{{- end -}}

{{/*
Generate effective service ports for headless service definition.
*/}}
{{- define "vector-aggregator.headlessServicePorts" -}}
{{- tuple true .Values | include "vector-aggregator.internalServicePorts" -}}
{{- end }}

{{/*
Determines whether there are any ports present.
*/}}
{{- define "vector-aggregator.servicePortsPresent" -}}
{{- or (or .Values.vectorSource.enabled (.Values.customConfig)) (not (empty .Values.service.ports)) }}
{{- end }}

{{/*
Generate an array of ServicePorts based on customConfig
*/}}
{{- define "vector-aggregator.generatePorts" -}}
{{- range $componentKind, $configs := .Values.customConfig }}
{{- if eq $componentKind "sources" }}
{{- range $componentId, $componentConfig := $configs }}
{{- if (hasKey $componentConfig "address") }}
{{- tuple $componentId $componentConfig | include "_helper.generatePort" -}}
{{- end }}
{{- end }}
{{- else if eq $componentKind "sinks" }}
{{- range $componentId, $componentConfig := $configs }}
{{- if (hasKey $componentConfig "address") }}
{{- tuple $componentId $componentConfig | include "_helper.generatePort" -}}
{{- end }}
{{- end }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Generate a single ServicePort based on a component configuration
*/}}
{{- define "_helper.generatePort" -}}
{{- $name := index . 0 | kebabcase -}}
{{- $config := index . 1 -}}
{{- $port := mustRegexFind "[0-9]+$" (get $config "address") -}}
{{- $protocol := default (get $config "mode" | upper) "TCP" }}
- name: {{ $name }}
  port: {{ $port }}
  protocol: {{ $protocol }}
  targetPort: {{ $port }}
{{- if not (mustHas $protocol (list "TCP" "UDP")) }}
{{ fail "Component's `mode` is not a supported protocol, please raise a issue at https://github.com/timberio/vector" }}
{{- end }}
{{- end }}
