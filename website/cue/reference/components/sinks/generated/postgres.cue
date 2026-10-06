package metadata

generated: components: sinks: postgres: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	batch: {
		description: """
			Event batching behavior.

			Note that as PostgreSQL's `jsonb_populate_recordset` function is used to insert events,
			a single event in the batch can make the whole batch to fail. For example, if a single event within the batch triggers
			a unique constraint violation in the destination table, the whole event batch will fail.

			As a workaround, [triggers](https://www.postgresql.org/docs/current/sql-createtrigger.html) on constraint violations
			can be defined at a database level to change the behavior of the insert operation on specific tables.
			Alternatively, setting `max_events` batch setting to `1` will make each event to be inserted independently,
			so events that trigger a constraint violation will not affect the rest of the events.
			"""
		required: false
		type:     _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::RealtimeSizeBasedDefaultBatchSettings>"]
	}
	endpoint: {
		description: """
			The PostgreSQL server connection string. It can contain the username and password.
			See [PostgreSQL documentation](https://www.postgresql.org/docs/current/libpq-connect.html#LIBPQ-CONNSTRING) about connection strings for more information
			about valid formats and options that can be used.
			"""
		required: true
		type: string: {}
	}
	pool_size: {
		description: """
			The postgres connection pool size. See [this](https://docs.rs/sqlx/latest/sqlx/struct.Pool.html#why-use-a-pool) for more
			information about why a connection pool should be used.
			"""
		required: false
		type: uint: default: 5
	}
	request: {
		description: """
			Middleware settings for outbound requests.

			Various settings can be configured, such as concurrency and rate limits, timeouts, and retry behavior.

			Note that the retry backoff policy follows the Fibonacci sequence.
			"""
		required: false
		type:     _schemaDefinitions["vector::sinks::util::service::TowerRequestConfig"]
	}
	table: {
		description: """
			The table that data is inserted into. This table parameter is vulnerable
			to SQL injection attacks as Vector does not validate or sanitize it, you must not use untrusted input.
			This parameter will be directly interpolated in the SQL query statement,
			as table names as parameters in prepared statements are not allowed in PostgreSQL.
			"""
		required: true
		type: string: {}
	}
}
