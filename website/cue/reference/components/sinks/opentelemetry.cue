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
		requirements: ["This sink excepts events conforming to the [OTEL proto format](\(urls.opentelemetry_proto)). You can use [Remap](\(urls.vector_remap_transform)) to prepare events for ingestion."]
		warnings: []
		notices: []
	}

	configuration: base.components.sinks.opentelemetry.configuration
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
				
							.timestamp_nanos = to_unix_timestamp!(syslog.timestamp, unit: "nanoseconds")
							.body = syslog
							.service_name = syslog.appname
							.resource_attributes.source_type = .source_type
							.resource_attributes.host.hostname = syslog.hostname
							.resource_attributes.service.name = syslog.appname
							.attributes.syslog.procid = syslog.procid
							.attributes.syslog.facility = syslog.facility
							.attributes.syslog.version = syslog.version
							.severity_text = if includes(["emerg", "err", "crit", "alert"], syslog.severity) {
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
							.scope_name = syslog.msgid
				
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
				
				"""
		}
	}
}
