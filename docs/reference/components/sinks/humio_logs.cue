package metadata

components: sinks: humio_logs: {
	title: "Humio Logs"
	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["Humio"]
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
			encoding: codec: {
				enabled: true
				default: null
				enum: ["json", "text"]
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
			tls: enabled: false
			to: {
				name:     "Humio"
				thing:    "a \(name) database"
				url:      urls.humio
				versions: null

				interface: {
					socket: {
						api: {
							title: "Humio Splunk HEC API"
							url:   urls.humio_hec
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "disabled"
					}
				}
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
		event_type: {
			common:      false
			description: "The type of events sent to this sink. Humio uses this as the name of the parser to use to ingest the data.\n\nIf unset, Humio will default it to none.\n"
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["json", "none"]
			}
		}
		host_key: {
			common:      true
			description: "The name of the log field to be used as the hostname sent to Humio. This overrides the [global `host_key` option][docs.reference.global-options#host_key]."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["hostname"]
			}
		}
		source: {
			common:      false
			description: "The source of events sent to this sink. Typically the filename the logs originated from. Maps to @source in Humio.\n"
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["{{file}}", "/var/log/syslog", "UDP:514"]
			}
		}
		token: {
			description: "Your Humio ingestion token."
			required:    true
			warnings: []
			type: string: {
				examples: ["${HUMIO_TOKEN}", "A94A8FE5CCB19BA61C4C08"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}
}
