package metadata

components: sinks: splunk_hec_logs: {
	title: "Splunk HEC logs"
	alias: "splunk_hec"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["Splunk"]
		stateful: false
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10_000_000
				timeout_secs: 1
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					enum: ["json", "text"]
				}
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: false
			}
			tls: {
				enabled:                true
				can_enable:             false
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
			}
			to: {
				service: services.splunk

				interface: {
					socket: {
						api: {
							title: "Splunk HEC event endpoint"
							url:   urls.splunk_hec_event_endpoint
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "optional"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: sinks._splunk_hec.configuration & {
		endpoint: {
			description: "The base URL of the Splunk instance."
			required:    true
			type: string: {
				examples: ["https://http-inputs-hec.splunkcloud.com", "https://hec.splunk.com:8088", "http://example.com"]
			}
		}
		host_key: {
			common:      true
			description: """
				The name of the log field to be used as the hostname sent to Splunk HEC. This overrides the
				[global `host_key` option](\(urls.vector_configuration)/global-options#log_schema.host_key).
				"""
			required:    false
			type: string: {
				default: null
				examples: ["hostname"]
			}
		}
		index: {
			common:      false
			description: "The name of the index where to send the events to. If not specified, the default index is used."
			required:    false
			type: string: {
				default: null
				examples: ["{{ host }}", "custom_index"]
				syntax: "template"
			}
		}
		indexed_fields: {
			common:      true
			description: "Fields to be [added to Splunk index](\(urls.splunk_hec_indexed_fields))."
			required:    false
			type: array: {
				default: null
				items: type: string: {
					examples: ["field1", "field2"]
					syntax: "field_path"
				}
			}
		}
		source: {
			common:      false
			description: "The source of events sent to this sink. Typically the filename the logs originated from. If unset, the Splunk collector will set it."
			required:    false
			type: string: {
				default: null
				examples: ["{{ file }}", "/var/log/syslog", "UDP:514"]
				syntax: "template"
			}
		}
		sourcetype: {
			common:      false
			description: "The sourcetype of events sent to this sink. If unset, Splunk will default to httpevent."
			required:    false
			type: string: {
				default: null
				examples: ["{{ sourcetype }}", "_json", "httpevent"]
				syntax: "template"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	telemetry: metrics: {
		component_sent_bytes_total:       components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		encode_errors_total:              components.sources.internal_metrics.output.metrics.encode_errors_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
		http_request_errors_total:        components.sources.internal_metrics.output.metrics.http_request_errors_total
		processing_errors_total:          components.sources.internal_metrics.output.metrics.processing_errors_total
		processed_bytes_total:            components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:           components.sources.internal_metrics.output.metrics.processed_events_total
		requests_received_total:          components.sources.internal_metrics.output.metrics.requests_received_total
	}

	how_it_works: sinks._splunk_hec.how_it_works
}
