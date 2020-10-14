package metadata

components: sinks: loki: {
	title:             "Loki"
	short_description: "Batches log events to [Loki][urls.loki]."
	long_description:  "[Loki][urls.loki] is a horizontally-scalable, highly-available, multi-tenant log aggregation system inspired by [Prometheus][urls.prometheus]. It is designed to be very cost effective and easy to operate. It does not index the contents of the logs, but rather a set of labels for each log stream."

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		function:      "transmit"
		service_providers: ["Grafana"]
	}

	features: {
		batch: {
			enabled:      true
			common:       false
			max_bytes:    102400
			max_events:   100000
			timeout_secs: 1
		}
		buffer: enabled:      true
		compression: enabled: false
		encoding: {
			enabled: true
			default: null
			json:    null
			ndjson:  null
			text:    null
		}
		healthcheck: enabled: true
		request: {
			enabled:                    true
			in_flight_limit:            5
			rate_limit_duration_secs:   1
			rate_limit_num:             5
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
	}

	support: {
		input_types: ["log"]

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
	}

	configuration: {
		auth: {
			common:      false
			description: "Options for the authentication strategy."
			required:    false
			warnings: []
			type: object: {
				examples: []
				options: {
					password: {
						description: "The basic authentication password. If using GrafanaLab's hosted Loki then this must be set to your `instanceId`."
						required:    true
						warnings: []
						type: string: {
							examples: ["${LOKI_PASSWORD}", "password"]
						}
					}
					strategy: {
						description: "The authentication strategy to use."
						required:    true
						warnings: []
						type: string: {
							enum: {
								basic:  "The [basic authentication strategy][urls.basic_auth]."
								bearer: "The bearer token authentication strategy."
							}
						}
					}
					token: {
						description: "The token to use for bearer authentication"
						required:    true
						warnings: []
						type: string: {
							examples: ["${API_TOKEN}", "xyz123"]
						}
					}
					user: {
						description: "The basic authentication user name. If using GrafanaLab's hosted Loki then this must be set to your Grafana.com api key."
						required:    true
						warnings: []
						type: string: {
							examples: ["${LOKI_USERNAME}", "username"]
						}
					}
				}
			}
		}
		labels: {
			description: "A set of labels that will be attached to each batch of events. These values are also templateable to allow events to provide dynamic label values.Note: If the set of label values has high cardinality this can cause drastic performance issues with Loki. To ensure this does not happen one should try to reduce the amount of unique label values."
			required:    true
			warnings: []
			type: object: {
				examples: [{"forwarder": "vector"}, {"event": "{{ event_field }}"}, {"key": "value"}]
				options: {}
			}
		}
		remove_label_fields: {
			common:      false
			description: "If this is set to `true` then when labels are collected from events those fields will also get removed from the event."
			required:    false
			warnings: []
			type: bool: default: false
		}
		remove_timestamp: {
			common:      false
			description: "If this is set to `true` then the timestamp will be removed from the event. This is useful because Loki uses the timestamp to index the event."
			required:    false
			warnings: []
			type: bool: default: true
		}
		tenant_id: {
			common:      false
			description: "The tenant id that will be sent with every request, by default this is not required since a proxy should set this header. When running Loki locally a tenant id is not required either.\n\nYou can read more about tenant id's [here][urls.loki_multi_tenancy]"
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["some_tenant_id"]
			}
		}
	}
}
