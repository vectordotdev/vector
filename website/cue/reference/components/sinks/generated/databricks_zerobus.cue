package metadata

generated: components: sinks: databricks_zerobus: configuration: {
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
			Databricks authentication configuration.

			See the [Databricks Zerobus documentation][zerobus_service_principal] to create a service
			principal and grant it permissions to write to the target table.

			[zerobus_service_principal]: https://docs.databricks.com/aws/en/ingestion/zerobus-ingest#create-a-service-principal-and-grant-permissions
			"""
		required: true
		type: object: options: {
			client_id: {
				description: "OAuth 2.0 client ID."
				required:    true
				type: string: examples: ["${DATABRICKS_CLIENT_ID}", "abc123..."]
			}
			client_secret: {
				description: "OAuth 2.0 client secret."
				required:    true
				type: string: examples: ["${DATABRICKS_CLIENT_SECRET}", "secret123..."]
			}
			strategy: {
				description: "The authentication strategy to use for Databricks."
				required:    true
				type: string: enum: oauth: "Authenticate using OAuth 2.0 client credentials."
			}
		}
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::RealtimeSizeBasedDefaultBatchSettings>"]
	}
	ingestion_endpoint: {
		description: """
			The Zerobus ingestion endpoint URL.

			This should be the full URL to the Zerobus ingestion service.

			See the [Databricks Zerobus documentation][zerobus_endpoint] to find your workspace URL and
			Zerobus ingest endpoint.

			[zerobus_endpoint]: https://docs.databricks.com/aws/en/ingestion/zerobus-ingest#get-your-workspace-url-and-zerobus-ingest-endpoint
			"""
		required: true
		type: string: examples: ["https://1234567890123456.zerobus.us-west-2.cloud.databricks.com", "https://6543210987654321.zerobus.us-east-1.cloud.databricks.com"]
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
	stream_options: {
		description: """
			Zerobus stream configuration options.

			This is a thin wrapper around the SDK's `StreamConfigurationOptions` with Vector-specific
			configuration attributes and custom defaults suitable for Vector's use case.
			"""
		required: false
		type: object: options: {
			compression: {
				description: "Arrow IPC compression for Flight payloads. Defaults to no compression."
				required:    false
				type: string: {
					default: "none"
					enum: {
						lz4_frame: "LZ4 frame compression."
						none:      "No compression."
						zstd:      "Zstandard compression."
					}
				}
			}
			flush_timeout_ms: {
				description: "Timeout in milliseconds for flush operations."
				required:    false
				type: uint: {
					default: 30000
					examples: [30000]
				}
			}
			server_lack_of_ack_timeout_ms: {
				description: "Timeout in milliseconds for server acknowledgements."
				required:    false
				type: uint: {
					default: 60000
					examples: [60000]
				}
			}
		}
	}
	table_name: {
		description: """
			The Unity Catalog table name to write to.

			This should be in the format `catalog.schema.table`.

			See the [Databricks Zerobus documentation][zerobus_table] to create or identify the target
			table.

			[zerobus_table]: https://docs.databricks.com/aws/en/ingestion/zerobus-ingest#create-or-identify-the-target-table
			"""
		required: true
		type: string: examples: ["main.default.logs", "main.default.vector_logs"]
	}
	unity_catalog_endpoint: {
		description: """
			The Unity Catalog endpoint URL.

			This is used for authentication and table metadata.

			See the [Databricks Zerobus documentation][zerobus_endpoint] to find your workspace URL and
			Zerobus ingest endpoint.

			[zerobus_endpoint]: https://docs.databricks.com/aws/en/ingestion/zerobus-ingest#get-your-workspace-url-and-zerobus-ingest-endpoint
			"""
		required: true
		type: string: examples: ["https://dbc-a1b2c3d4-e5f6.cloud.databricks.com", "https://dbc-f6e5d4c3-b2a1.cloud.databricks.com"]
	}
	user_agent: {
		description: """
			Custom identifier appended to the `user-agent` header sent to Databricks.

			The header always includes `Vector/<version>`; when set, this value is
			appended after it (e.g. `my-service/1.2`).
			"""
		required: false
		type: string: examples: ["my-service/1.2"]
	}
}
