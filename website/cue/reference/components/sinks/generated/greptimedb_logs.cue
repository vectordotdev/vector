package metadata

generated: components: sinks: greptimedb_logs: configuration: {
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
	compression: {
		description: """
			Set http compression encoding for the request
			Default to none, `gzip` or `zstd` is supported.
			"""
		required: false
		type: string: {
			default: "gzip"
			enum: {
				gzip: """
					[Gzip][gzip] compression.

					[gzip]: https://www.gzip.org/
					"""
				none: "No compression."
				snappy: """
					[Snappy][snappy] compression.

					[snappy]: https://github.com/google/snappy/blob/main/docs/README.md
					"""
				zlib: """
					[Zlib][zlib] compression.

					[zlib]: https://zlib.net/
					"""
				zstd: """
					[Zstandard][zstd] compression.

					[zstd]: https://facebook.github.io/zstd/
					"""
			}
		}
	}
	dangerously_allow_unconfined_template_resolution: {
		description: """
			Disable all template confinement checks for this sink.

			**DANGEROUS — disables a security control.**

			Bypasses both startup validation and runtime confinement for every
			templated field on this sink. When enabled, a log producer that
			controls any field used in a template can write to arbitrary keys,
			paths, or routing destinations. This flag is a full opt-out: it
			disables confinement even for templates that have a usable static
			prefix.
			"""
		required: false
		type: bool: default: false
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
			syntax: "template"
		}
	}
	encoding: {
		description: "Transformations to prepare an event for serialization."
		required:    false
		type:        _schemaDefinitions["codecs::encoding::transformer::Transformer"]
	}
	endpoint: {
		description: "The endpoint of the GreptimeDB server."
		required:    true
		type: string: examples: ["http://localhost:4000"]
	}
	extra_headers: {
		description: """
			Custom headers to add to the HTTP request sent to GreptimeDB.
			Note that these headers will override the existing headers.
			"""
		required: false
		type: object: options: "*": {
			description: "Extra header key-value pairs."
			required:    true
			type: string: {}
		}
	}
	extra_params: {
		description: "Custom parameters to add to the query string for each HTTP request sent to GreptimeDB."
		required:    false
		type: object: {
			examples: [{
				source: "vector"
			}]
			options: "*": {
				description: "A query string parameter."
				required:    true
				type: string: {}
			}
		}
	}
	password: {
		description: """
			The password for your GreptimeDB instance.

			This is required if your instance has authentication enabled.
			"""
		required: false
		type: string: examples: ["password"]
	}
	pipeline_name: {
		description: """
			Pipeline name to be used for the logs.

			Default to `greptime_identity`, use the original log structure
			"""
		required: false
		type: string: {
			default: "greptime_identity"
			examples: ["pipeline_name"]
			syntax: "template"
		}
	}
	pipeline_version: {
		description: "Pipeline version to be used for the logs."
		required:    false
		type: string: {
			examples: ["2024-06-07 06:46:23.858293"]
			syntax: "template"
		}
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
		description: "The table that data is inserted into."
		required:    true
		type: string: {
			examples: ["mytable"]
			syntax: "template"
		}
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
