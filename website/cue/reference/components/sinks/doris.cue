package metadata

components: sinks: doris: {
	title: "Doris"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["Apache"]
		stateful: false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10_000_000
				timeout_secs: 1.0
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["none", "gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: enabled: false
			}
			proxy: enabled: false
			request: {
				enabled: true
				headers: true
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      true
			}
			to: {
				service: services.doris

				interface: {
					socket: {
						api: {
							title: "Doris Stream Load API"
							url:   urls.doris_stream_load
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
			#"""
				Doris version 1.0 or higher is required for optimal compatibility.
				"""#,
		]
		warnings: []
		notices: []
	}

	configuration: generated.components.sinks.doris.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		stream_load: {
			title: "Stream Load"
			body:  """
				Vector uses Doris's [Stream Load](\(urls.doris_stream_load)) API to efficiently
				ingest data. Stream Load is Doris's primary method for real-time data ingestion,
				providing high throughput and low latency.

				Each batch of events is sent as a single Stream Load request with a unique label
				to ensure exactly-once semantics. The label is generated using the configured
				`label_prefix` and a timestamp-based suffix.
				"""
		}

		batching: {
			title: "Batching"
			body: """
				Vector batches events before sending them to Doris to improve throughput and
				reduce the number of Stream Load requests. The batching behavior is controlled
				by the `batch` configuration options:

				- `max_events`: Maximum number of events per batch
				- `max_bytes`: Maximum size of a batch in bytes
				- `timeout_secs`: Maximum time to wait before flushing a partial batch

				When any of these limits is reached, the batch is flushed to Doris.
				"""
		}

		authentication: {
			title: "Authentication"
			body: """
				Vector supports HTTP basic authentication for connecting to Doris. The
				credentials are configured using the `auth.user` and `auth.password` options.

				The authentication is performed on each Stream Load request to the Doris
				Frontend (FE) nodes.
				"""
		}

		error_handling: {
			title: "Error Handling"
			body: """
				Vector implements comprehensive error handling for Doris Stream Load operations:

				- **Retries**: Failed requests are automatically retried based on the
				  `max_retries` configuration. Set to `-1` for unlimited retries.
				- **Backoff**: Exponential backoff is used between retry attempts to avoid
				  overwhelming the Doris cluster.
				- **Partial Failures**: If a Stream Load request fails due to data format
				  issues, Vector logs the error and continues processing subsequent batches.
				- **Connection Failures**: Network-level failures trigger automatic failover
				  to other configured endpoints if available.
				"""
		}

		load_balancing: {
			title: "Load Balancing and Failover"
			body: """
				When multiple endpoints are configured, Vector automatically distributes
				Stream Load requests across all available Doris Frontend nodes. This provides
				both load balancing and high availability:

				- **Round-robin distribution**: Requests are distributed evenly across endpoints
				- **Health monitoring**: Unhealthy endpoints are automatically excluded
				- **Automatic failover**: If an endpoint becomes unavailable, traffic is
				  redirected to healthy endpoints
				- **Recovery**: Previously failed endpoints are periodically retested and
				  re-included when they become healthy again
				"""
		}

		data_format: {
			title: "Data Format"
			body: """
				Vector sends data to Doris in JSON format by default. Each event is serialized
				as a JSON object, and multiple events are sent as newline-delimited JSON (NDJSON).

				The data format can be customized using the `headers` configuration to set
				Doris-specific Stream Load parameters such as:

				- `format`: Data format (json, csv, etc.)
				- `read_json_by_line`: Whether to read JSON line by line
				- `strip_outer_array`: Whether to strip outer array brackets
				- Column mappings and transformations

				Example headers configuration:
				```yaml
				headers:
				  format: "json"
				  read_json_by_line: "true"
				  strip_outer_array: "false"
				```
				"""
		}

		exactly_once: {
			title: "Exactly-Once Semantics"
			body: """
				Vector ensures exactly-once delivery to Doris through the use of unique labels
				for each Stream Load request. Each label is generated using:

				- The configured `label_prefix`
				- A timestamp component
				- A unique identifier for the batch

				Doris uses these labels to detect and reject duplicate Stream Load requests,
				ensuring that data is not duplicated even if Vector retries a request.

				Labels follow the format: `{label_prefix}_{timestamp}_{batch_id}`
				"""
		}
	}
}
