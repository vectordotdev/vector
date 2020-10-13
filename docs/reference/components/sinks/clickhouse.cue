package metadata

components: sinks: clickhouse: {
	title:             "Clickhouse"
	short_description: "Batches log events to [Clickhouse][urls.clickhouse] via the [`HTTP` Interface][urls.clickhouse_http]."
	long_description:  "[ClickHouse][urls.clickhouse] is an open-source column-oriented database management system that manages extremely large volumes of data, including non-aggregated data, in a stable and sustainable manner and allows generating custom data reports in real time. The system is linearly scalable and can be scaled up to store and process trillions of rows and petabytes of data. This makes it an best-in-class storage for logs and metrics data."

	classes: {
		commonly_used: true
		egress_method: "batch"
		function:      "transmit"
		service_providers: ["Yandex"]
	}

	features: {
		batch: {
			enabled:      true
			common:       false
			max_bytes:    1049000
			max_events:   null
			timeout_secs: 1
		}
		buffer: enabled: true
		compression: {
			enabled: true
			default: "gzip"
			gzip:    true
		}
		encoding: codec: enabled: false
		healthcheck: enabled: true
		request: {
			enabled:                    true
			in_flight_limit:            5
			rate_limit_duration_secs:   1
			rate_limit_num:             5
			retry_initial_backoff_secs: 1
			retry_max_duration_secs:    10
			timeout_secs:               30
		}
		tls: {
			enabled:                true
			can_enable:             false
			can_verify_certificate: true
			can_verify_hostname:    true
			enabled_default:        false
		}
	}

	statuses: {
		delivery:    "at_least_once"
		development: "beta"
	}

	support: {
		platforms: {
			triples: {
				"aarch64-unknown-linux-gnu":  true
				"aarch64-unknown-linux-musl": true
				"x86_64-apple-darwin":        true
				"x86_64-pc-windows-msv":      true
				"x86_64-unknown-linux-gnu":   true
				"x86_64-unknown-linux-musl":  true
			}
		}

		requirements: [
			"""
				[Clickhouse][urls.clickhouse] version `>= 1.1.54378` is required.
				""",
		]
		warnings: []
		notices: []
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
						description: "The basic authentication password."
						required:    true
						warnings: []
						type: string: {
							examples: ["${CLICKHOUSE_PASSWORD}", "password"]
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
						description: "The basic authentication user name."
						required:    true
						warnings: []
						type: string: {
							examples: ["${CLICKHOUSE_USERNAME}", "username"]
						}
					}
				}
			}
		}
		database: {
			common:      true
			description: "The database that contains the stable that data will be inserted into."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["mydatabase"]
			}
		}
		endpoint: {
			description: "The endpoint of the [Clickhouse][urls.clickhouse] server."
			required:    true
			type: string: {
				examples: ["http://localhost:8123"]
			}
		}
		table: {
			description: "The table that data will be inserted into."
			required:    true
			warnings: []
			type: string: {
				examples: ["mytable"]
			}
		}
	}

	input: {
		logs:    true
		metrics: false
	}
}
