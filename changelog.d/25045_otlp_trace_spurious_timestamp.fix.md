The OTLP codec no longer injects a spurious `timestamp` field into trace events when `log_namespace` is `Legacy`. The legacy timestamp injection only applies to logs; trace events now always deserialize under `LogNamespace::Vector`.

authors: kimjune01
