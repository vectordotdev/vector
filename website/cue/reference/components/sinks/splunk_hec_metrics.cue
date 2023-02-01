package metadata

components: sinks: splunk_hec_metrics: {
	title: "Splunk HEC metrics"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["Splunk"]
		stateful: false
	}

	features: {
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10_000_000
				timeout_secs: 1.0
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: enabled: false
			proxy: enabled:    true
			request: {
				enabled: true
				headers: false
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      true
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
		default_namespace: {
			common: false
			description: """
				Used as a namespace for metrics that don't have it.
				A namespace will be prefixed to a metric's name.
				"""
			required: false
			type: string: {
				default: null
				examples: ["service"]
			}
		}
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
        				The name of the field to be used as the hostname sent to Splunk HEC. This overrides the
        				[global `host_key` option](\(urls.vector_configuration)/global-options#log_schema.host_key).
        				"""
			required:    false
			type: string: {
				default: null
				examples: ["hostname"]
			}
		}
		index: {
			common:      true
			description: "The name of the index where send the events to. If not specified, the default index is used."
			required:    false
			type: string: {
				default: null
				examples: ["{{ host }}", "custom_index"]
				syntax: "template"
			}
		}
		source: {
			common:      true
			description: "The source of events sent to this sink. If unset, the Splunk collector will set it."
			required:    false
			type: string: {
				default: null
				examples: ["{{ file }}", "/var/log/syslog", "UDP:514"]
				syntax: "template"
			}
		}
		sourcetype: {
			common:      true
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
		logs: false
		metrics: {
			counter:      true
			distribution: false
			gauge:        true
			histogram:    false
			set:          false
			summary:      false
		}
		traces: false
	}

	telemetry: components.sinks.splunk_hec_logs.telemetry

	how_it_works: sinks._splunk_hec.how_it_works & {
		multi_value_tags: {
			title: "Multivalue Tags"
			body: """
				If Splunk receives a tag with multiple values it will only take the last value specified,
				so Vector only sends this last value.
				"""
		}}
}
