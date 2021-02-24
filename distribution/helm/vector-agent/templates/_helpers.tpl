{{/* vim: set filetype=mustache: */}}

{{/*
Resolve effective address to use at the built-in vector sink.
*/}}
{{- define "vector-agent.vectorSinkAddress" -}}
{{- $host := required "You must specify the `vectorSink.host` for the built-in vector sink host" .Values.vectorSink.host }}
{{- $port := required "You must specify the `vectorSink.port` for the built-in vector sink port" .Values.vectorSink.port }}
{{- printf "%s:%s" $host (toString $port) }}
{{- end }}
