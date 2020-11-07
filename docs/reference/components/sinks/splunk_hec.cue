package metadata

components: sinks: splunk_hec: {
	title:       "Splunk HEC"
	description: "The [Splunk HTTP Event Collector (HEC)][urls.splunk_hec] is a fast and efficient way to send data to Splunk Enterprise and Splunk Cloud. Notably, HEC enables you to send data over HTTP (or HTTPS) directly to Splunk Enterprise or Splunk Cloud from your application."

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["Splunk"]
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
				in_flight_limit:            10
				rate_limit_duration_secs:   1
				rate_limit_num:             10
				retry_initial_backoff_secs: 1
				retry_max_duration_secs:    10
				timeout_secs:               60
			}
			tls: {
				enabled:                true
				can_enable:             false
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
			}
			to: {
				name:     "Splunk"
				thing:    "a \(name) index"
				url:      urls.splunk
				versions: null

				interface: {
					socket: {
						api: {
							title: "Splunk HEC protocol"
							url:   urls.splunk_hec_protocol
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "optional"
					}
				}

				setup: [
					"""
						Follow the [Splunk HEC setup docs][urls.splunk_hec_setup]
						and create a Splunk HEC endpoint.
						""",
					"""
						Splunk will provide you with a host and token. Copy those
						values to the `host` and `token` options.
						""",
				]
			}
		}
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		host_key: {
			common:      true
			description: "The name of the log field to be used as the hostname sent to Splunk HEC. This overrides the [global `host_key` option][docs.reference.global-options#host_key]."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["hostname"]
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
			}
		}
		indexed_fields: {
			common:      true
			description: "Fields to be [added to Splunk index][urls.splunk_hec_indexed_fields]."
			required:    false
			warnings: []
			type: array: {
				default: null
				items: type: string: examples: ["field1", "field2"]
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
			}
		}
		token: {
			description: "Your Splunk HEC token."
			required:    true
			warnings: []
			type: string: {
				examples: ["${SPLUNK_HEC_TOKEN}", "A94A8FE5CCB19BA61C4C08"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	telemetry: metrics: {
		vector_encode_errors_total:           _vector_encode_errors_total
		vector_http_request_errors_total:     _vector_http_request_errors_total
		vector_http_requests_total:           _vector_http_requests_total
		vector_source_missing_keys_total:     _vector_source_missing_keys_total
		vector_sourcetype_missing_keys_total: _vector_sourcetype_missing_keys_total
	}
}
