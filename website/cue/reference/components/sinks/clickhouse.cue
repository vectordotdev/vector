package metadata

components: sinks: clickhouse: {
	title: "Clickhouse"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["Yandex"]
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
				default: "gzip"
				algorithms: ["none", "gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: enabled: false
			}
			proxy: enabled: true
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
				service: services.clickhouse

				interface: {
					socket: {
						api: {
							title: "Clickhouse HTTP API"
							url:   urls.clickhouse_http
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
		requirements: [
			"""
				[Clickhouse](\(urls.clickhouse)) version `>= 1.1.54378` is required.
				""",
		]
		warnings: []
		notices: []
	}

	configuration: {
		auth: configuration._http_auth & {_args: {
			password_example: "${CLICKHOUSE_PASSWORD}"
			username_example: "${CLICKHOUSE_USERNAME}"
		}}
		database: {
			common:      true
			description: "The database that contains the table that data will be inserted into."
			required:    false
			type: string: {
				default: null
				examples: ["mydatabase"]
			}
		}
		endpoint: {
			description: "The endpoint of the [Clickhouse](\(urls.clickhouse)) server."
			required:    true
			type: string: {
				examples: ["http://localhost:8123", "tcp://localhost:9223"]
			}
		}
		use_native_proto: {
			description: "If `true`, ClickHouse Native Protocol is used. Defaults to `false`, using `JSONEachRow` over HTTP."
			required:    false
			type: bool: default: false
		}
		table: {
			description: "The table that data will be inserted into."
			required:    true
			type: string: {
				examples: ["mytable"]
			}
		}
		sql_table_col_def: {
			description: """
				The clickhouse table column definition.
				If `use_native_proto` is `true`, this field must be configured!
				The key represents not only the column name of clickhouse table but also the key of the log.
				The value currently only supports these types:
				_type: UInt(8,16,32,64), Int(8,16,32,64), String, FixedString(Int), Float(32,64), Date, DateTime, IPv(4,6),
				Array(_type), Nullable(_type), Map(String, _type).

				Note: for now, empty space is not acceptable in type definition, which means:
				Map(String,UInt8), Nullable(Date) are valid.
				Map( String, UInt8), Nullable(Date ) are invalid.
				"""
			required: false
			type: object: {
				examples: [
					{
						"name":     "String"
						"age":      "UInt8"
						"hobbites": "Map(String,String)"
					},
				]
				options: {
					"*": {
						common:      false
						description: "clickhouse table definition"
						required:    false
						type: string: {
							default: null
							examples: ["String", "Map(String,Date)", "Array(Int64)"]
						}
					}
				}
			}
		}
		skip_unknown_fields: {
			common:      true
			description: "Sets `input_format_skip_unknown_fields`, allowing Clickhouse to discard fields not present in the table schema."
			required:    false
			type: bool: default: false
		}
	}

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	telemetry: metrics: {
		component_sent_bytes_total:       components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
	}
}
