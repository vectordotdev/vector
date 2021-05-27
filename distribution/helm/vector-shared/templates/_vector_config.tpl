{{/* vim: set filetype=mustache: */}}

{{/*
Serialize the passed Vector component configuration bits as YAML.
*/}}
{{- define "libvector.vectorComponentConfig" -}}
{{- $componentGroup := index . 0 -}}
{{- $componentId := index . 1 -}}
{{- $value := index . 2 -}}

{{- /* Build the right hierarchy and evaluate the YAML. */ -}}
{{- $yaml := toYaml (dict $componentGroup (dict $componentId $value)) -}}
{{- /* Cut the root-level key containing the component kind name (i.e. `sinks`). */ -}}
{{- $yaml = $yaml | trimPrefix (printf "%s:\n" $componentGroup) -}}
{{- /* Cut tailing newline. */ -}}
{{- $yaml = $yaml | trimSuffix "\n" -}}
{{- /* Print the value. */ -}}
{{- $yaml -}}

{{- printf "\n" -}}
{{- end }}

{{/*
Serialize the passed Vector source configuration bits as YAML.
*/}}
{{- define "libvector.vectorSourceConfig" -}}
{{- $componentId := index . 0 -}}
{{- $value := index . 1 -}}
{{- tuple "sources" $componentId $value | include "libvector.vectorComponentConfig" -}}
{{- end }}

{{/*
Serialize the passed Vector transform configuration bits as YAML.
*/}}
{{- define "libvector.vectorTransformConfig" -}}
{{- $componentId := index . 0 -}}
{{- $value := index . 1 -}}
{{- tuple "transforms" $componentId $value | include "libvector.vectorComponentConfig" -}}
{{- end }}

{{/*
Serialize the passed Vector sink configuration bits as YAML.
*/}}
{{- define "libvector.vectorSinkConfig" -}}
{{- $componentId := index . 0 -}}
{{- $value := index . 1 -}}
{{- tuple "sinks" $componentId $value | include "libvector.vectorComponentConfig" -}}
{{- end }}

{{/*
Serialize the passed Vector topology configuration bits as YAML.
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

data_dir: "{{ .Values.globalOptions.dataDir }}"

api:
  enabled: {{ .Values.vectorApi.enabled }}
  address: {{ .Values.vectorApi.address | quote }}
  playground: {{ .Values.vectorApi.playground }}
{{- printf "\n" -}}

{{- with .Values.logSchema }}
log_schema:
  host_key: "{{ .hostKey }}"
  message_key: "{{ .messageKey }}"
  source_type_key: "{{ .sourceTypeKey }}"
  timestamp_key: "{{ .timestampKey }}"
  {{- printf "\n" -}}
{{- end }}
{{- end }}
