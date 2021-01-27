{{/* vim: set filetype=mustache: */}}

{{/*
Serialize the passed Vector component configuration bits as TOML.
*/}}
{{- define "libvector.vectorComponentConfig" -}}
{{- $componentGroup := index . 0 -}}
{{- $componentId := index . 1 -}}
{{- $value := index . 2 -}}

{{- $rawConfig := $value.rawConfig -}}
{{- $value = unset $value "rawConfig" -}}

{{- $header := printf "[%s.%s]" $componentGroup $componentId -}}

{{- /* Build the right hierarchy and evaluate the TOML. */ -}}
{{- $toml := toToml (dict $componentGroup (dict $componentId $value)) -}}
{{- /* Cut the root-level key containing the component kind name (i.e. `[sinks]`). */ -}}
{{- $toml = $toml | trimPrefix (printf "[%s]\n" $componentGroup) -}}
{{- /* Remove one level of indentation. */ -}}
{{- $toml = regexReplaceAllLiteral "(?m)^  " $toml "" -}}
{{- /* Cut tailing newline. */ -}}
{{- $toml = $toml | trimSuffix "\n" -}}
{{- /* Print the value. */ -}}
{{- $toml -}}

{{- with $rawConfig -}}
{{- /* Here is a poor attempt to ensure raw config section is put under the */ -}}
{{- /* component-level section. What we're trying to do here is prohibited */ -}}
{{- /* in the TOML spec, but it may work in the simple case - and this is */ -}}
{{- /* what we have to support for the backward compatibility. */ -}}
{{- if contains (printf "[%s.%s." $componentGroup $componentId) $toml -}}
{{- $header| nindent 0 -}}
{{- end -}}
{{- /* Print the raw config. */ -}}
  {{- $rawConfig | nindent 2 -}}
{{- end }}

{{- printf "\n" -}}
{{- end }}

{{/*
Serialize the passed Vector source configuration bits as TOML.
*/}}
{{- define "libvector.vectorSourceConfig" -}}
{{- $componentId := index . 0 -}}
{{- $value := index . 1 -}}
{{- tuple "sources" $componentId $value | include "libvector.vectorComponentConfig" -}}
{{- end }}

{{/*
Serialize the passed Vector transform configuration bits as TOML.
*/}}
{{- define "libvector.vectorTransformConfig" -}}
{{- $componentId := index . 0 -}}
{{- $value := index . 1 -}}
{{- tuple "transforms" $componentId $value | include "libvector.vectorComponentConfig" -}}
{{- end }}

{{/*
Serialize the passed Vector sink configuration bits as TOML.
*/}}
{{- define "libvector.vectorSinkConfig" -}}
{{- $componentId := index . 0 -}}
{{- $value := index . 1 -}}
{{- tuple "sinks" $componentId $value | include "libvector.vectorComponentConfig" -}}
{{- end }}

{{/*
Serialize the passed Vector topology configuration bits as TOML.
*/}}
{{- define "libvector.vectorTopology" -}}
{{- range $componentId, $value := .sources }}
{{- tuple $componentId $value | include "libvector.vectorSourceConfig" | nindent 0 -}}
{{- end }}

{{- range $componentId, $value := .transforms }}
{{- tuple $componentId $value | include "libvector.vectorTransformConfig" | nindent 0 -}}
{{- end }}

{{- range $componentId, $value := .sinks }}
{{- tuple $componentId $value | include "libvector.vectorSinkConfig" | nindent 0 -}}
{{- end }}
{{- end }}

{{/*
The common header for Vector ConfigMaps.
*/}}
{{- define "libvector.vectorConfigHeader" -}}
# Configuration for vector.
# Docs: https://vector.dev/docs/

data_dir = "{{ .Values.globalOptions.dataDir }}"

[api]
  enabled = {{ .Values.vectorApi.enabled }}
  address = {{ .Values.vectorApi.address | quote }}
  playground = {{ .Values.vectorApi.playground }}
{{- printf "\n" -}}

{{- with .Values.logSchema }}
[log_schema]
  host_key = "{{ .hostKey }}"
  message_key = "{{ .messageKey }}"
  source_type_key = "{{ .sourceTypeKey }}"
  timestamp_key = "{{ .timestampKey }}"
  {{- printf "\n" -}}
{{- end }}
{{- end }}
