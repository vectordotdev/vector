package metadata

_schemaDefinitions: "vector::sinks::util::service::TowerRequestConfig": object: options: {
	adaptive_concurrency: {
		description: """
			Configuration of adaptive concurrency parameters.

			These parameters typically do not require changes from the default, and incorrect values can lead to meta-stable or
			unstable performance and sink behavior. Proceed with caution.
			"""
		required: false
		type:     _schemaDefinitions["vector::sinks::util::adaptive_concurrency::AdaptiveConcurrencySettings"]
	}
	concurrency: {
		description: """
			Configuration for outbound request concurrency.

			This can be set either to one of the below enum values or to a positive integer, which denotes
			a fixed concurrency limit.
			"""
		required: false
		type: {
			string: {
				default: "adaptive"
				enum: {
					adaptive: """
						Concurrency is managed by Vector's [Adaptive Request Concurrency][arc] feature.

						[arc]: https://vector.dev/docs/architecture/arc/
						"""
					none: """
						A fixed concurrency of 1.

						Only one request can be outstanding at any given time.
						"""
				}
			}
			uint: {}
		}
	}
	rate_limit_duration_secs: {
		description: "The time window used for the `rate_limit_num` option."
		required:    false
		type: uint: {
			default: 1
			unit:    "seconds"
		}
	}
	rate_limit_num: {
		description: "The maximum number of requests allowed within the `rate_limit_duration_secs` time window."
		required:    false
		type: uint: {
			default: 9223372036854775807
			unit:    "requests"
		}
	}
	retry_attempts: {
		description: "The maximum number of retries to make for failed requests."
		required:    false
		type: uint: {
			default: 9223372036854775807
			unit:    "retries"
		}
	}
	retry_initial_backoff_secs: {
		description: """
			The amount of time to wait before attempting the first retry for a failed request.

			After the first retry has failed, the Fibonacci sequence is used to select future backoffs.
			"""
		required: false
		type: uint: {
			default: 1
			unit:    "seconds"
		}
	}
	retry_jitter_mode: {
		description: "The jitter mode to use for retry backoff behavior."
		required:    false
		type: string: {
			default: "Full"
			enum: {
				Full: """
					Full jitter.

					The random delay is anywhere from 0 up to the maximum current delay calculated by the backoff
					strategy.

					Incorporating full jitter into your backoff strategy can greatly reduce the likelihood
					of creating accidental denial of service (DoS) conditions against your own systems when
					many clients are recovering from a failure state.
					"""
				None: "No jitter."
			}
		}
	}
	retry_max_duration_secs: {
		description: "The maximum amount of time to wait between retries."
		required:    false
		type: uint: {
			default: 30
			unit:    "seconds"
		}
	}
	timeout_secs: {
		description: """
			The time a request can take before being aborted.

			Datadog highly recommends that you do not lower this value below the service's internal timeout, as this could
			create orphaned requests, pile on retries, and result in duplicate data downstream.
			"""
		required: false
		type: uint: {
			default: 60
			unit:    "seconds"
		}
	}
}
