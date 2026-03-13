package metadata

components: sinks: databricks_zerobus: {
	title: "Databricks Zerobus"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["Databricks"]
		stateful: false
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10_000_000
				timeout_secs: 1.0
			}
			compression: enabled: false
			encoding: enabled:    false
			proxy: enabled:       false
			request: {
				enabled: true
				headers: false
			}
			tls: enabled: true
			to: {
				service: services.databricks_zerobus

				interface: {
					socket: {
						api: {
							title: "Databricks Zerobus Ingestion API"
							url:   urls.databricks
						}
						direction: "outgoing"
						protocols: ["https"]
						ssl: "required"
					}
				}
			}
		}
	}

	support: {
		requirements: [
			"""
				A [Databricks](\(urls.databricks)) workspace with [Unity Catalog](\(urls.databricks_unity_catalog)) enabled.
				""",
			"""
				OAuth 2.0 client credentials (client ID and client secret) with permissions to write to the target table.
				""",
		]
		warnings: []
		notices: []
	}

	configuration: generated.components.sinks.databricks_zerobus.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		authentication: {
			title: "Authentication"
			body: """
				The Databricks Zerobus sink authenticates using OAuth 2.0 client credentials.
				You must provide a `client_id` and `client_secret` that have been granted
				permissions to write to the target Unity Catalog table.
				"""
		}

		schema: {
			title: "Schema"
			body: """
				The sink requires a schema to encode events into protobuf format. The schema can
				be provided in two ways:

				#### Unity Catalog (default)

				When `schema.type` is set to `unity_catalog`, the sink automatically fetches the
				table schema from the Unity Catalog API at startup. This is the recommended approach
				as it ensures the schema always matches the target table.

				```yaml
				sinks:
				  zerobus:
				    type: databricks_zerobus
				    schema:
				      type: unity_catalog
				```

				#### Protobuf descriptor file

				You can provide a pre-compiled protobuf descriptor file. This is useful for
				development or when the Unity Catalog API is not accessible.

				```yaml
				sinks:
				  zerobus:
				    type: databricks_zerobus
				    schema:
				      type: path
				      path: /path/to/schema.desc
				      message_type: package.MessageName
				```

				Descriptor files can be generated using protoc:

				```sh
				protoc --descriptor_set_out=schema.desc --include_imports your_schema.proto
				```
				"""
		}

		batching: {
			title: "Batching"
			body: """
				Events are batched before being sent to Zerobus. Each event is individually
				serialized as a protobuf message, and the batch is sent as a single request.
				The maximum batch size is 10MB, which is enforced by the Zerobus SDK.
				"""
		}

		error_handling: {
			title: "Error Handling & Retries"
			body: """
				The sink classifies errors from the Zerobus SDK into retryable and non-retryable
				categories:

				- **Retryable errors** (connection failures, stream closed, channel errors): The
				  sink automatically discards the current gRPC stream and creates a fresh one on
				  the next retry attempt. This ensures recovery from transient network issues
				  without manual intervention.

				- **Non-retryable errors** (invalid table, invalid endpoint, invalid arguments):
				  Events are rejected permanently and the existing stream is kept alive.

				Retry behavior (backoff, concurrency, timeouts) is controlled by the standard
				`request` configuration options.
				"""
		}

		acknowledgements: {
			title: "Acknowledgements"
			body: """
				When `acknowledgements` is enabled, the sink waits for a server-side
				acknowledgement after each batch is ingested. This confirms that the Zerobus
				service has received and accepted the data before marking events as delivered.

				When disabled (the default), events are marked as delivered as soon as the
				ingestion call completes without error, without waiting for an explicit
				server acknowledgement.
				"""
		}
	}
}
