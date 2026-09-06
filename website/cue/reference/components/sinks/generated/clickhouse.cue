package metadata

generated: components: sinks: clickhouse: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	auth: {
		description: """
			Configuration of the authentication strategy for HTTP requests.

			HTTP authentication should be used with HTTPS only, as the authentication credentials are passed as an
			HTTP header without any additional encryption beyond what is provided by the transport itself.
			"""
		required: false
		type:     _schemaDefinitions["core::option::Option<vector::http::Auth>"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::RealtimeSizeBasedDefaultBatchSettings>"]
	}
	batch_encoding: {
		description: """
			The batch encoding configuration for encoding events in batches.

			When specified, events are encoded together as a single batch.
			This is mutually exclusive with per-event encoding based on the `format` field.
			"""
		required: false
		type: object: options: {
			allow_nullable_fields: {
				description: """
					Allow null values for non-nullable fields in the schema.

					When enabled, missing or incompatible values are encoded as null, even for fields
					marked as non-nullable in the Arrow schema. This is useful when working with downstream
					systems that can handle null values through defaults, computed columns, or other mechanisms.

					When disabled (default), missing values for non-nullable fields results in encoding errors. This is to
					help ensure all required data is present before sending it to the sink.
					"""
				required: false
				type: bool: default: false
			}
			codec: {
				description: """
					Encodes events in [Apache Arrow][apache_arrow] IPC streaming format.

					This is the streaming variant of the Arrow IPC format, which writes
					a continuous stream of record batches.

					[apache_arrow]: https://arrow.apache.org/
					"""
				required: true
				type: string: enum: arrow_stream: """
					Encodes events in [Apache Arrow][apache_arrow] IPC streaming format.

					This is the streaming variant of the Arrow IPC format, which writes
					a continuous stream of record batches.

					[apache_arrow]: https://arrow.apache.org/
					"""
			}
		}
	}
	compression: {
		description: """
			Compression configuration.

			All compression algorithms use the default compression level unless otherwise specified.
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
	database: {
		description: "The database that contains the table that data is inserted into."
		required:    false
		type: string: {
			examples: ["mydatabase"]
			syntax: "template"
		}
	}
	date_time_best_effort: {
		description: "Sets `date_time_input_format` to `best_effort`, allowing ClickHouse to properly parse RFC3339/ISO 8601."
		required:    false
		type: bool: default: false
	}
	encoding: {
		description: "Transformations to prepare an event for serialization."
		required:    false
		type:        _schemaDefinitions["codecs::encoding::transformer::Transformer"]
	}
	endpoint: {
		description: "The endpoint of the ClickHouse server."
		required:    true
		type: string: examples: ["http://localhost:8123"]
	}
	format: {
		description: """
			Data format.

			The format to parse input data.
			"""
		required: false
		type: string: {
			default: "json_each_row"
			enum: {
				arrow_stream:   "ArrowStream (beta)."
				json_as_object: "JSONAsObject."
				json_as_string: "JSONAsString."
				json_each_row:  "JSONEachRow."
			}
		}
	}
	insert_random_shard: {
		description: "Sets `insert_distributed_one_random_shard`, allowing ClickHouse to insert data into a random shard when using Distributed Table Engine."
		required:    false
		type: bool: default: false
	}
	query_settings: {
		description: "Query settings for the `clickhouse` sink."
		required:    false
		type: object: options: async_insert_settings: {
			description: "Async insert-related settings."
			required:    false
			type: object: options: {
				deduplicate: {
					description: """
						Sets `async_insert_deduplicate`, allowing ClickHouse to perform deduplication when inserting blocks in the replicated table.

						If left unspecified, use the default provided by the `ClickHouse` server.
						"""
					required: false
					type: bool: {}
				}
				enabled: {
					description: """
						Sets `async_insert`, allowing ClickHouse to queue the inserted data and later flush to table in the background.

						If left unspecified, use the default provided by the `ClickHouse` server.
						"""
					required: false
					type: bool: {}
				}
				max_data_size: {
					description: """
						Sets `async_insert_max_data_size`, the maximum size in bytes of unparsed data collected per query before being inserted.

						If left unspecified, use the default provided by the `ClickHouse` server.
						"""
					required: false
					type: uint: {}
				}
				max_query_number: {
					description: """
						Sets `async_insert_max_query_number`, the maximum number of insert queries before being inserted

						If left unspecified, use the default provided by the `ClickHouse` server.
						"""
					required: false
					type: uint: {}
				}
				wait_for_processing: {
					description: """
						Sets `wait_for`, allowing ClickHouse to wait for processing of asynchronous insertion.

						If left unspecified, use the default provided by the `ClickHouse` server.
						"""
					required: false
					type: bool: {}
				}
				wait_for_processing_timeout: {
					description: """
						Sets 'wait_for_processing_timeout`, to control the timeout for waiting for processing asynchronous insertion.

						If left unspecified, use the default provided by the `ClickHouse` server.
						"""
					required: false
					type: uint: {}
				}
			}
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
	skip_unknown_fields: {
		description: """
			Sets `input_format_skip_unknown_fields`, allowing ClickHouse to discard fields not present in the table schema.

			If left unspecified, use the default provided by the `ClickHouse` server.
			"""
		required: false
		type: bool: {}
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
}
