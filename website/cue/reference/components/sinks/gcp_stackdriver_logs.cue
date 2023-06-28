package metadata

components: sinks: gcp_stackdriver_logs: {
	title: "GCP Operations (formerly Stackdriver) Logs"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["GCP"]
		stateful: false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10_000_000
				timeout_secs: 1.0
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: false
			}
			proxy: enabled: true
			request: {
				enabled:        true
				rate_limit_num: 1000
				headers:        false
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			to: {
				service: services.gcp_operations_logs

				interface: {
					socket: {
						api: {
							title: "REST Interface"
							url:   urls.gcp_stackdriver_logging_rest
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
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

	configuration: base.components.sinks.gcp_stackdriver_logs.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		severity_level_mapping: {
			title: "Severity Level Mapping"
			body:  """
				If a `severity_key` is configured, outgoing log records have their
				`severity` header field set from the named field in the Vector
				event. However, the [required values](\(urls.gcp_stackdriver_severity)) for
				this field may be inconvenient to produce, typically requiring a custom
				mapping using an additional transform. To assist with this, this sink
				remaps certain commonly used words to the required numbers as in the
				following table. Note that only the prefix is compared, such that a
				value of `emergency` matches `emerg`, and the comparison ignores case.

				| Prefix   | Value
				|:---------|:-----
				| `emerg`  | 800
				| `fatal`  | 800
				| `alert`  | 700
				| `crit`   | 600
				| `err`    | 500
				| `warn`   | 400
				| `notice` | 300
				| `info`   | 200
				| `debug`  | 100
				| `trace`  | 100
				"""
		}
	}

	permissions: iam: [
		{
			platform: "gcp"
			_service: "logging"

			policies: [
				{
					_action: "logEntries.create"
					required_for: ["healthcheck", "operation"]
				},
			]
		},
	]
}
