package metadata

generated: components: sinks: azure_logs_ingestion: configuration: {
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
		description: "Azure service principal authentication."
		required:    true
		type:        _schemaDefinitions["vector::sinks::azure_common::config::AzureAuthentication"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::RealtimeSizeBasedDefaultBatchSettings>"]
	}
	dcr_immutable_id: {
		description: """
			The [Data collection rule immutable ID][dcr_immutable_id] for the Data collection endpoint.

			[dcr_immutable_id]: https://learn.microsoft.com/en-us/azure/azure-monitor/logs/logs-ingestion-api-overview
			"""
		required: true
		type: string: examples: ["dcr-000a00a000a00000a000000aa000a0aa"]
	}
	encoding: {
		description: "Transformations to prepare an event for serialization."
		required:    false
		type:        _schemaDefinitions["codecs::encoding::transformer::Transformer"]
	}
	endpoint: {
		description: """
			The [Data collection endpoint URI][endpoint] associated with the Log Analytics workspace.

			[endpoint]: https://learn.microsoft.com/en-us/azure/azure-monitor/logs/logs-ingestion-api-overview
			"""
		required: true
		type: string: examples: ["https://my-dce-5kyl.eastus-1.ingest.monitor.azure.com"]
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
	retry_strategy: {
		description: """
			Configurable retry strategy for `http` based sinks.

			For more information about error responses, see [Client Error Responses][error_responses].

			[error_responses]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status#client_error_responses
			"""
		required: false
		type: object: options: {
			status_codes: {
				description:   "Retry on these specific HTTP status codes"
				relevant_when: "type = \"custom\""
				required:      true
				type: array: items: type: uint: {}
			}
			type: {
				description: "The retry strategy enum."
				required:    false
				type: string: {
					default: "default"
					enum: {
						all:     "Retry on *all* HTTP status codes except for success codes (2xx)"
						custom:  "Custom retry strategy"
						default: "Default strategy. See [`RetryStrategy::retry_action`] for more details."
						none:    "Don't retry any errors, including request timeouts."
					}
				}
			}
		}
	}
	stream_name: {
		description: """
			The [Stream name][stream_name] for the Data collection rule.

			[stream_name]: https://learn.microsoft.com/en-us/azure/azure-monitor/logs/logs-ingestion-api-overview
			"""
		required: true
		type: string: examples: ["Custom-MyTable"]
	}
	timestamp_field: {
		description: """
			The destination field (column) for the timestamp.

			The setting of `log_schema.timestamp_key`, usually `timestamp`, is used as the source.
			Most schemas use `TimeGenerated`, but some use `Timestamp` (legacy) or `EventStartTime` (ASIM) [std_columns].

			[std_columns]: https://learn.microsoft.com/en-us/azure/azure-monitor/logs/log-standard-columns#timegenerated
			"""
		required: false
		type: string: {
			default: "TimeGenerated"
			examples: ["EventStartTime", "Timestamp"]
		}
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
	token_scope: {
		description: """
			[Token scope][token_scope] for dedicated Azure regions.

			[token_scope]: https://learn.microsoft.com/en-us/azure/azure-monitor/logs/logs-ingestion-api-overview
			"""
		required: false
		type: string: {
			default: "https://monitor.azure.com/.default"
			examples: ["https://monitor.azure.us/.default", "https://monitor.azure.cn/.default"]
		}
	}
}
