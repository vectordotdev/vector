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
			The path to the file where the last row of the result set will be saved.
			The last row of the result set is saved in JSON format.
			This file provides parameters for the SQL query in the next scheduled run.
			If the file does not exist or the path is not specified, the initial value from `statement_init_params` is used.

			Tracking metadata is written only after all result batches are converted and sent.
			If saving fails after events were already sent, the previous tracking values are kept
			and the next scheduled run may emit duplicate rows.

			Parent directories are created automatically if they do not exist.

			# Examples

			If `tracking_columns = ["id", "name"]`, it is saved as the following JSON data.

			```json
			{"id":1, "name": "vector"}
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
			Maximum string length for ODBC driver operations.
			Set to `0` to omit the upper bound and use driver-reported sizes instead.
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
			Initial parameters for the first execution of the statement.
			Used if `last_run_metadata_path` does not exist.
			Values must be strings and follow the parameter order defined in the query.

			# Examples

			When the source runs for the first time, the file at `last_run_metadata_path` does not exist.
			In that case, declare the initial values in `statement_init_params`.

			```yaml
			sources:
			  odbc:
			    statement: "SELECT * FROM users WHERE id = ?"
			    statement_init_params:
			      id: "0"
			    tracking_columns:
			      - id
			    last_run_metadata_path: /path/to/tracking.json
			    # The rest of the fields are omitted
			```
			"""
		required: false
		type: object: options: "*": {
			description: "Initial value for the SQL statement parameters. The value is always a string."
			required:    true
			type: "*": {}
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
			Their values are passed as parameters to the SQL statement in the next scheduled run.

			Tracking metadata is saved only after all result batches are converted and sent.
			If a run fails partway through, the previous tracking values are kept so rows are
			not skipped on the next scheduled run.

			Requires `statement_init_params` or `last_run_metadata_path` so the first scheduled
			run has values to bind.

			# Examples

			```yaml
			sources:
			  odbc:
			    statement: "SELECT * FROM users WHERE id = ?"
			    tracking_columns:
			      - id
			    # The rest of the fields are omitted
			```
			"""
		required: false
		type: array: items: type: string: examples: ["id"]
	}
}
