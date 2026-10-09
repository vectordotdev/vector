package metadata

components: sinks: opentelemetry: {
	title: "Open Telemetry"

	classes: {
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
		requirements: ["""
			With `encoding.codec: otlp`, native Vector logs and metrics are converted to the
			[OTEL proto format](\(urls.opentelemetry_proto)), and events that already have the OTLP
			structure (for example from the `opentelemetry` source with `use_otlp_decoding`) are sent as
			they are. Trace events must already have the OTLP structure. With other codecs, you can use
			[Remap](\(urls.vector_remap_transform)) to prepare events for ingestion.
			"""]
		warnings: [
			"""
				Batching only works with `encoding.codec: otlp`, which encodes events as protobuf
				and supports native batching (recommended). The legacy `encoding.codec: json` path
				produces newline-delimited JSON, which is not a valid OTLP request body, so on
				that path you must either set `batch.max_events: 1` or merge events into a single
				envelope upstream with the [`reduce`](\(urls.vector_reduce_transform)) transform.
				See [#22054](https://github.com/vectordotdev/vector/issues/22054).
				""",
		]
	}

	configuration: generated.components.sinks.opentelemetry.configuration
	how_it_works: {
		otlp_root_fields: {
			title: "Events with the OTLP structure"
			body: """
				With `encoding.codec: otlp`, the root fields `resourceLogs`, `resourceMetrics`, and
				`resourceSpans` are reserved. A log event with one of these fields is treated as an
				OTLP request that is already built:

				| Root field | Encoded as |
				| --- | --- |
				| `resourceLogs` | `ExportLogsServiceRequest` |
				| `resourceMetrics` | `ExportMetricsServiceRequest` |
				| `resourceSpans` | `ExportTraceServiceRequest` |

				If an event has more than one of these fields, the first field in the table is used.
				Only the fields of that request are encoded, and all other event fields are not sent.
				If the value does not have the OTLP structure, the encoding fails.

				Vector does not examine the value to find if it is OTLP data. For example, this event
				is sent as an empty OTLP logs request, and `message` is not sent:

				```yaml
				message: finished checking resources
				resourceLogs: []
				```

				To send an event like this as a log record, rename the field before the sink, for
				example with a `remap` transform.
				"""
		}
		native_log_conversion: {
			title: "Native log conversion"
			body: """
				With `encoding.codec: otlp`, a log event without a `resourceLogs`, `resourceMetrics`,
				or `resourceSpans` root field is converted to one OTLP log record. The conversion is
				the inverse of the `opentelemetry` source decoding, so logs that the source decodes
				without `use_otlp_decoding` are sent back with the same log record, resource
				attributes, and scope, with these exceptions:

				- The source does not keep the resource and scope `schemaUrl` or the resource
				  `droppedAttributesCount`, so these fields are empty.
				- A `bytesValue` body or attribute that is valid UTF-8 is sent as a `stringValue`.
				- A record without `timeUnixNano` is sent with `timeUnixNano` set to the observed
				  time, because the source uses the observed time as the event timestamp.

				To send OTLP logs from the source exactly as received, use `use_otlp_decoding` on the
				source.

				| Event field (Legacy namespace) | OTLP field |
				| --- | --- |
				| `message` | `body` |
				| `timestamp` | `timeUnixNano` |
				| `observed_timestamp` | `observedTimeUnixNano` |
				| `attributes` | `attributes` |
				| `resources` | `resource.attributes` |
				| `scope.name`, `scope.version`, `scope.attributes`, `scope.dropped_attributes_count` | `scope` |
				| `trace_id`, `span_id` (hex strings) | `traceId`, `spanId` |
				| `severity_text`, `severity_number` | `severityText`, `severityNumber` |
				| `flags`, `dropped_attributes_count` | `flags`, `droppedAttributesCount` |

				With the Vector log namespace, the event becomes the `body` and the other fields are
				read from the `opentelemetry` source metadata. If that metadata is missing, the field
				with the `timestamp` meaning gives `timeUnixNano`, and the Vector ingest timestamp
				gives `observedTimeUnixNano`.

				All other event fields are sent as log record attributes. This includes a mapped field
				that does not have the type OTLP requires, for example a `trace_id` that is not 32 hex
				characters. If a key is both in `attributes` and at the top level, the value from
				`attributes` is used. The `source_type` field is not sent.
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
