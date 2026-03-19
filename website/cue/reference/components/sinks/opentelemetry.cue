package metadata

components: sinks: opentelemetry: {
	title: "Open Telemetry"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: enabled: false
	}

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: true
	}

	support: {
		requirements: ["With `codec: otlp`, native Vector logs, traces, and metrics are automatically converted to OTLP protobuf format. Pre-formatted OTLP events (from `use_otlp_decoding: true`) are passed through unchanged. Native metrics use tag prefix decomposition (`resource.*`, `scope.*`) to reconstruct the original OTLP resource/scope/data-point attribute hierarchy."]
		warnings: []
		notices: []
	}

	configuration: generated.components.sinks.opentelemetry.configuration
	how_it_works: {
		metric_tag_prefixes: {
			title: "Metric tag prefix conventions"
			body: """
				When encoding native Vector metrics with `codec: otlp`, the following tag prefixes are reserved and control
				how tags are mapped into the OTLP protobuf structure:

				- `resource.*` — Stripped of prefix and placed into `Resource.attributes[]` (e.g. `resource.service.name` becomes attribute `service.name`)
				- `resource_dropped_attributes_count` — Mapped to `Resource.dropped_attributes_count` (not an attribute)
				- `resource_schema_url` — Mapped to `ResourceMetrics.schema_url` (not an attribute)
				- `scope.name` — Mapped to `InstrumentationScope.name`
				- `scope.version` — Mapped to `InstrumentationScope.version`
				- `scope_dropped_attributes_count` — Mapped to `InstrumentationScope.dropped_attributes_count` (not an attribute)
				- `scope_schema_url` — Mapped to `ScopeMetrics.schema_url` (not an attribute)
				- `scope.*` (other) — Stripped of prefix and placed into `InstrumentationScope.attributes[]`

				All other tags are added to the data point `attributes[]` array unchanged.

				This means native metrics that use `resource.*` or `scope.*` tag names for non-OTLP purposes are routed to the OTLP resource and scope structures rather than remaining as flat data point attributes.
				This is the expected behavior when round-tripping OTLP metrics through Vector, but may be surprising for metrics
				from non-OTLP sources that coincidentally use these prefixes.

				**Known limitations:**

				- Metric attribute types are preserved during OTLP→Vector→OTLP roundtrip via a typed metadata sidecar.
				  All OTLP value kinds (`StringValue`, `BytesValue`, `IntValue`, `BoolValue`, `DoubleValue`, `ArrayValue`,
				  `KvlistValue`) are stored with their kind wrapper and reconstructed on encode. If a VRL transform mutates
				  metric tags, the sidecar is invalidated and all attributes fall back to `StringValue`.
				- `start_time_unix_nano` is preserved for OTLP-sourced metrics using metadata stash. For native Vector
				  incremental metrics, `start_time_unix_nano` is set to `timestamp - interval_ms` when available, otherwise set to `0`.
				"""
		}
		quickstart: {
			title: "Quickstart"
			body: """
				This sink is a wrapper over the HTTP sink. The following is an example of how you can push OTEL logs to an OTEL collector.

				1. The Vector config:

				```yaml
				sources:
					generate_syslog:
						type: "demo_logs"
						format: "syslog"
						count: 100000
						interval: 1

				transforms:
					remap_syslog:
						inputs: ["generate_syslog"]
						type: "remap"
						source: |
							syslog = parse_syslog!(.message)

							severity_text = if includes(["emerg", "err", "crit", "alert"], syslog.severity) {
								"ERROR"
							} else if syslog.severity == "warning" {
								"WARN"
							} else if syslog.severity == "debug" {
								"DEBUG"
							} else if includes(["info", "notice"], syslog.severity) {
								"INFO"
							} else {
								syslog.severity
							}

							.resourceLogs = [{
								"resource": {
									"attributes": [
										{ "key": "source_type", "value": { "stringValue": .source_type } },
										{ "key": "service.name", "value": { "stringValue": syslog.appname } },
										{ "key": "host.hostname", "value": { "stringValue": syslog.hostname } }
									]
								},
								"scopeLogs": [{
									"scope": {
										"name": syslog.msgid
									},
									"logRecords": [{
										"timeUnixNano": to_unix_timestamp!(syslog.timestamp, unit: "nanoseconds"),
										"body": { "stringValue": syslog.message },
										"severityText": severity_text,
										"attributes": [
											{ "key": "syslog.procid", "value": { "stringValue": to_string(syslog.procid) } },
											{ "key": "syslog.facility", "value": { "stringValue": syslog.facility } },
											{ "key": "syslog.version", "value": { "stringValue": to_string(syslog.version) } }
										]
									}]
								}]
							}]

							del(.message)
							del(.timestamp)
							del(.service)
							del(.source_type)

				sinks:
					emit_syslog:
						inputs: ["remap_syslog"]
						type: opentelemetry
						protocol:
							type: http
							uri: http://localhost:5318/v1/logs
							method: post
							encoding:
								codec: json
							framing:
								method: newline_delimited
							headers:
								content-type: application/json
				```

				2. Sample OTEL collector config:

				```yaml
				receivers:
					otlp:
						protocols:
							http:
								endpoint: "0.0.0.0:5318"

				exporters:
					debug:
						verbosity: detailed
					otlp:
						endpoint: localhost:4317
						tls:
							insecure: true

				processors:
					batch: {}

				service:
					pipelines:
						logs:
							receivers: [otlp]
							processors: [batch]
							exporters: [debug]
				```

				3. Run the OTEL instance:

				```sh
				./otelcol --config ./otel/config.yaml
				```

				4. Run Vector:

				```sh
				VECTOR_LOG=debug cargo run -- --config /path/to/vector/config.yaml
				```

				In the console for the OTEL Collector you can see the logs and their contents as they come in.

				Here's an example of a JSON payload you might see from Vector:

				```json
				{
				  "host": "localhost",
				  "resourceLogs": [
					{
					  "resource": {
						"attributes": [
						  {
							"key": "source_type",
							"value": {
							  "stringValue": "demo_logs"
							}
						  },
						  {
							"key": "service.name",
							"value": {
							  "stringValue": "shaneIxD"
							}
						  },
						  {
							"key": "host.hostname",
							"value": {
							  "stringValue": "random.org"
							}
						  }
						]
					  },
					  "scopeLogs": [
						{
						  "logRecords": [
							{
							  "attributes": [
								{
								  "key": "syslog.procid",
								  "value": {
									"stringValue": "7906"
								  }
								},
								{
								  "key": "syslog.facility",
								  "value": {
									"stringValue": "local0"
								  }
								},
								{
								  "key": "syslog.version",
								  "value": {
									"stringValue": "1"
								  }
								}
							  ],
							  "body": {
								"stringValue": "Maybe we just shouldn't use computers"
							  },
							  "severityText": "WARN",
							  "timeUnixNano": 1737045415051000000
							}
						  ],
						  "scope": {
							"name": "ID856"
						  }
						}
					  ]
					}
				  ]
				}
				```

				"""
		}
	}
}
