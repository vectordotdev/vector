package metadata

generated: components: sinks: greptimedb_metrics: configuration: {
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
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::greptimedb::GreptimeDBDefaultBatchSettings>"]
	}
	dbname: {
		description: """
			The [GreptimeDB database][database] name to connect.

			Default to `public`, the default database of GreptimeDB.

			Database can be created via `create database` statement on
			GreptimeDB. If you are using GreptimeCloud, use `dbname` from the
			connection information of your instance.

			[database]: https://docs.greptime.com/user-guide/concepts/key-concepts#database
			"""
		required: false
		type: string: {
			default: "public"
			examples: [
				"public"
			]
		}
	}
	endpoint: {
		description: """
			The host and port of GreptimeDB gRPC service.

			This sink uses GreptimeDB's gRPC interface for data ingestion. By
			default, GreptimeDB listens to port 4001 for gRPC protocol.

			The address _must_ include a port.
			"""
		required: true
		type: string: examples: ["example.com:4001"]
	}
	grpc_compression: {
		description: "Set gRPC compression encoding for the request."
		required:    false
		type: string: {
			default: "none"
			enum: {
				gzip: "Gzip compression."
				none: "No compression."
				zstd: "Zstandard compression."
			}
		}
	}
	new_naming: {
		description: """
			Use Greptime's prefixed naming for time index and value columns.

			This is to keep consistency with GreptimeDB's naming pattern. By
			default, this sink will use `val` for value column name, and `ts` for
			time index name. When turned on, `greptime_value` and
			`greptime_timestamp` will be used for these names.

			If you are using this Vector sink together with other data ingestion
			sources of GreptimeDB, like Prometheus Remote Write and Influxdb Line
			Protocol, it is highly recommended to turn on this.

			Also if there is a tag name conflict from your data source, for
			example, you have a tag named as `val` or `ts`, you need to turn on
			this option to avoid the conflict.

			Default to `false` for compatibility.
			"""
		required: false
		type: bool: {}
	}
	password: {
		description: """
			The password for your GreptimeDB instance.

			This is required if your instance has authentication enabled.
			"""
		required: false
		type: string: examples: ["password"]
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
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
	username: {
		description: """
			The username for your GreptimeDB instance.

			This is required if your instance has authentication enabled.
			"""
		required: false
		type: string: examples: ["username"]
	}
}
