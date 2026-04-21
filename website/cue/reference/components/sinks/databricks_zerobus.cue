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
			proxy: enabled:       true
			request: {
				enabled: true
				headers: false
			}
			tls: enabled: false
			to: {
				service: services.databricks_zerobus

				interface: {
					socket: {
						api: {
							title: "Databricks Zerobus Ingestion API"
							url:   urls.databricks
						}
						direction: "outgoing"
						protocols: ["http"]
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
				The sink requires a schema to encode events into protobuf format.

				The sink automatically fetches the table schema from the Unity Catalog API
				at startup using the configured `table_name` and `unity_catalog_endpoint`,
				ensuring the schema always matches the target table. No additional schema
				configuration is required.
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

		proxy: {
			title: "Proxy"
			body: """
				Both the Zerobus gRPC ingestion channel and the Unity Catalog schema
				discovery requests honor Vector's `proxy` configuration (`proxy.http`,
				`proxy.https`, `proxy.no_proxy`), which itself is merged with the standard
				`HTTP_PROXY`, `HTTPS_PROXY`, and `NO_PROXY` environment variables.

				Because the Zerobus endpoint is always HTTPS gRPC, the `proxy.https` URL is
				used when set; `proxy.http` is used as a fallback only if `proxy.https` is
				not configured. Hosts matching `proxy.no_proxy` bypass the proxy. Both
				`http://` and `https://` proxy URIs are supported — for HTTPS proxies, the
				client-to-proxy hop does its own TLS handshake using the system trust store.

				Setting `proxy.enabled = false` disables proxying entirely, including the
				SDK's built-in env-var fallback.
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
