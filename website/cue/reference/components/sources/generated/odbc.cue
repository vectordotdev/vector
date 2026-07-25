package metadata

generated: components: sources: odbc: configuration: {
	connection_string: {
		description: """
			The connection string to use for ODBC.
			If the `connection_string_filepath` is set, this value is ignored.
			"""
		required: false
		type: string: {
			default: ""
			examples: ["driver={MariaDB Unicode};server=<ip or host>;port=<port number>;database=<database name>;uid=<user>;pwd=<password>"]
		}
	}
	connection_string_filepath: {
		description: """
			The path to the file that contains the connection string.
			If this is not set or the file at that path does not exist, the `connection_string` field is used instead.
			"""
		required: false
		type: string: examples: ["driver={MariaDB Unicode};server=<ip or host>;port=<port number>;database=<database name>;uid=<user>;pwd=<password>"]
	}
	last_run_metadata_path: {
		description: """
			The path to the file where tracked column values will be saved.
			The tracked values are saved in JSON format and overlaid onto `statement_init_params`
			for the next scheduled run.
			If the file does not exist or the path is not specified, the initial values from
			`statement_init_params` are used.

			When tracking is enabled, the full query result is buffered, the final-row checkpoint
			is validated and written here, and only then are events emitted. A missing or
			unbindable tracking value fails the poll before any emit (avoiding infinite replay).
			A send failure after a successful checkpoint write may skip those rows on the next
			run (at-most-once). The in-memory overlay is also advanced in that case so tracking
			without `last_run_metadata_path` does not replay already-emitted rows. Prefer
			incremental queries so the buffered result stays bounded.

			Parent directories are created automatically if they do not exist.

			# Examples

			If `tracking_columns = ["id", "name"]`, it is saved as the following JSON data.

			```json
			{"id":"42","name":"vector"}
			```
			"""
		required: false
		type: string: examples: ["/path/to/tracking.json"]
	}
	login_timeout: {
		description: """
			Maximum time to wait for the ODBC connection/login to complete.
			If the connection does not succeed within this window, the attempt fails
			and is retried at the next scheduled run.
			Set to 0 to disable the timeout and wait indefinitely.
			Prefer a positive timeout: Vector shutdown waits for any in-flight connect/execute, and
			`0` can delay exit until the ODBC driver returns.
			The default is 3 seconds.
			"""
		required: false
		type: uint: {
			default: 3
			examples: [
				3,
			]
			unit: "seconds"
		}
	}
	odbc_batch_size: {
		description: """
			Number of rows to fetch, convert, and send per batch.
			This bounds ODBC driver fetch buffers and in-memory processing for each batch.
			Must be greater than 0.
			The default is 100.
			"""
		required: false
		type: uint: {
			default: 100
			examples: [
				100,
			]
		}
	}
	odbc_default_timezone: {
		description: """
			Timezone applied to database date/time columns that lack timezone information.
			Ambiguous DST times use the latest matching instant; nonexistent times are kept as text.
			Offset-bearing SQL or RFC3339 timestamp text is preserved as bytes and is not rewritten
			with this timezone, so tracking parameters can round-trip the exact ODBC text.
			The default is UTC.
			"""
		required: false
		type: string: {
			default: "UTC"
			examples: [
				"UTC",
			]
		}
	}
	odbc_max_str_limit: {
		description: """
			Maximum bytes per cell when allocating ODBC text and binary fetch buffers.
			Caps driver-reported sizes. Set to `0` to omit the upper bound and use
			driver-reported sizes instead.
			The default is 4096.
			"""
		required: false
		type: uint: {
			default: 4096
			examples: [
				4096,
			]
		}
	}
	schedule: {
		description: "Cron expression used to schedule database queries. This field is required."
		required:    true
		type: string: {}
	}
	schedule_timezone: {
		description: """
			The timezone to use for the `schedule`.
			Typically the timezone used when evaluating the cron expression.
			The default is UTC.

			[Wikipedia]: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
			"""
		required: false
		type: string: {
			default: "UTC"
			examples: [
				"UTC",
			]
		}
	}
	statement: {
		description: """
			The SQL statement to execute.
			This SQL statement is executed periodically according to the `schedule`.
			Defaults to `None`. If no SQL statement is provided, the source returns an error.
			If the `statement_filepath` is set, this value is ignored.
			"""
		required: false
		type: string: examples: ["SELECT * FROM users WHERE id = ?"]
	}
	statement_filepath: {
		description: """
			The path to the file that contains the SQL statement.
			If this is set, the `statement` field is ignored and the file must exist and be readable.
			"""
		required: false
		type: string: {}
	}
	statement_init_params: {
		description: """
			Positional parameters for SQL statement placeholders (`?`).

			Array order is the bind order. Static filter values and tracking bootstrap
			values can be mixed; only names listed in `tracking_columns` are overlaid
			from checkpoints or the previous result.

			# Examples

			Incremental query with a static tenant filter:

			```yaml
			sources:
			  odbc:
			    statement: "SELECT * FROM users WHERE tenant_id = ? AND id > ? ORDER BY id ASC"
			    statement_init_params:
			      - name: tenant_id
			        value: "acme"
			      - name: id
			        value: "0"
			    tracking_columns:
			      - id
			    last_run_metadata_path: /path/to/tracking.json
			    # The rest of the fields are omitted
			```

			Static-only filter without tracking:

			```yaml
			sources:
			  odbc:
			    statement: "SELECT * FROM users WHERE tenant_id = ?"
			    statement_init_params:
			      - name: tenant_id
			        value: "acme"
			    # The rest of the fields are omitted
			```
			"""
		required: false
		type: array: items: type: object: options: {
			name: {
				description: """
					Parameter name.

					When the same name appears in `tracking_columns`, later runs overlay the
					checkpointed or last-row value onto this entry while preserving bind order.
					"""
				required: true
				type: string: examples: ["id", "tenant_id"]
			}
			value: {
				description: """
					Initial value bound for this placeholder.

					For non-tracking parameters this value is reused on every scheduled run.
					For tracking parameters it is used until a checkpoint or previous result
					provides an updated value.
					"""
				required: true
				type: string: examples: ["0", "acme"]
			}
		}
	}
	statement_timeout: {
		description: """
			Maximum time to allow the SQL statement to run.
			If the query does not finish within this window, it is canceled and retried at the next scheduled run.
			Set to 0 to disable the timeout and wait indefinitely.
			Prefer a positive timeout: Vector shutdown waits for any in-flight connect/execute, and
			`0` can delay exit until the ODBC driver returns.
			The default is 3 seconds.
			"""
		required: false
		type: uint: {
			default: 3
			examples: [
				3,
			]
			unit: "seconds"
		}
	}
	tracking_columns: {
		description: """
			Specifies the columns to track from the last row of the statement result set.
			Their values overlay matching entries in `statement_init_params` on later runs while
			preserving the declared bind order.

			When set, result batches are buffered until the query finishes; the final-row
			checkpoint is validated (and persisted when `last_run_metadata_path` is set) before
			any events are emitted. That avoids replaying the same rows when the last row is
			missing a tracking column or has an unbindable value such as null.
			Prefer incremental/`WHERE` bounded queries so buffering stays memory-safe.

			Requires `statement_init_params` entries whose names cover every tracking column.
			Optional `last_run_metadata_path` overlays checkpointed values onto those entries.
			Prefer non-binary tracking columns: checkpoints bind text parameters, so raw
			`BINARY`/`VARBINARY`/`BYTEA` values that are not valid UTF-8 fail validation.

			# Examples

			```yaml
			sources:
			  odbc:
			    statement: "SELECT * FROM users WHERE id > ? ORDER BY id ASC"
			    statement_init_params:
			      - name: id
			        value: "0"
			    tracking_columns:
			      - id
			    # The rest of the fields are omitted
			```
			"""
		required: false
		type: array: items: type: string: examples: ["id"]
	}
}
