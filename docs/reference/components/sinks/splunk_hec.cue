package metadata

components: sinks: splunk_hec: {
	title: "Splunk HEC"

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
				max_bytes:    1049000
				max_events:   null
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
					default: null
					enum: ["json", "text"]
				}
			}
			request: {
				enabled:                    true
				concurrency:                10
				rate_limit_duration_secs:   1
				rate_limit_num:             10
				retry_initial_backoff_secs: 1
				retry_max_duration_secs:    10
				timeout_secs:               60
				headers:                    false
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
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		endpoint: {
			description: "The base URL of the Splunk instance."
			required:    true
			type: string: {
				examples: ["https://http-inputs-hec.splunkcloud.com", "https://hec.splunk.com:8088", "http://example.com"]
				syntax: "literal"
			}
		}
		host_key: {
			common:      true
			description: "The name of the log field to be used as the hostname sent to Splunk HEC. This overrides the [global `host_key` option][docs.reference.configuration.global-options#host_key]."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["hostname"]
				syntax: "literal"
			}
		}
		index: {
			common:      false
			description: "The name of the index where send the events to. If not specified, the default index is used.\n"
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["custom_index"]
				syntax: "literal"
			}
		}
		indexed_fields: {
			common:      true
			description: "Fields to be [added to Splunk index][urls.splunk_hec_indexed_fields]."
			required:    false
			warnings: []
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
			description: "The source of events sent to this sink. Typically the filename the logs originated from. If unset, the Splunk collector will set it.\n"
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["/var/log/syslog", "UDP:514"]
				syntax: "literal"
			}
		}
		sourcetype: {
			common:      false
			description: "The sourcetype of events sent to this sink. If unset, Splunk will default to httpevent.\n"
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["_json", "httpevent"]
				syntax: "literal"
			}
		}
		token: {
			description: "Your Splunk HEC token."
			required:    true
			warnings: []
			type: string: {
				examples: ["${SPLUNK_HEC_TOKEN}", "A94A8FE5CCB19BA61C4C08"]
				syntax: "literal"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	telemetry: metrics: {
		encode_errors_total:       components.sources.internal_metrics.output.metrics.encode_errors_total
		http_request_errors_total: components.sources.internal_metrics.output.metrics.http_request_errors_total
		missing_keys_total:        components.sources.internal_metrics.output.metrics.missing_keys_total
		processed_bytes_total:     components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:    components.sources.internal_metrics.output.metrics.processed_events_total
		requests_received_total:   components.sources.internal_metrics.output.metrics.requests_received_total
	}
}
