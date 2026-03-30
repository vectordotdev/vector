`opentelemetry` source: Implemented header enrichment for OTLP metrics and traces. Unlike logs, which support enriching
the event itself or its metadata, depending on `log_namespace` settings, for metrics and traces this setting is ignored
and header values are added to the event metadata.

Issue: https://github.com/vectordotdev/vector/issues/24619

authors: ozanichkovsky
