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
		requirements: ["This sink accepts events conforming to the [OTEL proto format](\(urls.opentelemetry_proto)). You can use [Remap](\(urls.vector_remap_transform)) to prepare events for ingestion."]
		warnings: []
		notices: []
	}

	configuration: generated.components.sinks.opentelemetry.configuration
	how_it_works: {
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
