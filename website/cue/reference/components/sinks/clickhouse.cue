package metadata

components: sinks: clickhouse: {
	title: "ClickHouse"

	classes: {
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["Yandex"]
		stateful: false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
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
							title: "ClickHouse HTTP API"
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
				[ClickHouse](\(urls.clickhouse)) version `>= 1.1.54378` is required.
				""",
		]
		warnings: []
	}

	configuration: generated.components.sinks.clickhouse.configuration

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
		traces: false
	}

	how_it_works: {
		data_formats: {
			title: "Data Formats"
			body: """
				The ClickHouse sink supports multiple data formats for inserting events:

				#### JSONEachRow (default)

				The default format is `JSONEachRow`, which sends events as newline-delimited JSON. Each event is
				encoded as a single JSON object on its own line. This format is simple and flexible, allowing
				ClickHouse to handle type conversions automatically.

				```yaml
				sinks:
				clickhouse:
					type: clickhouse
					endpoint: http://localhost:8123
					database: my_database
					table: my_table
					format: json_each_row  # default
				```

				#### JSONAsObject and JSONAsString

				These formats provide alternative JSON encoding strategies:
				- `json_as_object`: Wraps the entire event as a JSON object
				- `json_as_string`: Encodes the event as a JSON string

				#### ArrowStream (beta)

				The `arrow_stream` format uses Apache Arrow's streaming format to send data to ClickHouse. This
				format offers better performance and type safety by fetching the table schema from ClickHouse at
				startup and encoding events directly into Arrow format.

				```yaml
				sinks:
				clickhouse:
					type: clickhouse
					endpoint: http://localhost:8123
					database: my_database
					table: my_table
					format: arrow_stream
					batch_encoding:
						codec: arrow_stream
				```

				**Note**: The ArrowStream format requires a static (non-templated) table and database name, as the
				schema is fetched once at startup. Dynamic table routing is not supported with this format.
				"""
		}

		arrow_type_mappings: {
			title: "Arrow Type Mappings"
			body:  """
				When using the `arrow_stream` format, Vector automatically converts ClickHouse types to Arrow types.
				The sink fetches the table schema from ClickHouse and maps each column type accordingly.

				#### Unsupported ClickHouse Types

				The following ClickHouse column types are **not yet supported** by Vector's
				ArrowStream implementation:
				- `IPv4`
				- `IPv6`

				If your table contains these types, you should use one of the JSON formats instead.

				#### Unsupported Arrow Types

				Based on [ClickHouse's Arrow format documentation](\(urls.clickhouse_arrow)), the following
				types are unsupported:
				- `FIXED_SIZE_BINARY`
				- `JSON`
				- `ENUM`

				#### Timezone Handling

				DateTime and DateTime64 columns with timezone information will be converted to Arrow timestamps
				without timezone metadata. All timestamps are treated as UTC by default.
				"""
		}
		metrics: {
			title: "Metrics"
			body:  """
				With the JSON formats, metric events are encoded as one JSON object per row, using
				Vector's native metric shape. For example, a counter is sent as:

				```json
				{"name":"requests_total","namespace":"app","tags":{"host":"a"},"timestamp":"2025-01-01T00:00:00Z","kind":"incremental","counter":{"value":2.0}}
				```

				Vector does not enforce a table schema for metrics, so create a table whose columns match the
				fields you want to keep, and enable `skip_unknown_fields` to drop the rest. Enable
				`date_time_best_effort` to parse the RFC 3339 `timestamp` into a `DateTime64` column.

				Metrics are not supported when `batch_encoding` is set (`arrow_stream` format).
				"""
		}
	}
}
