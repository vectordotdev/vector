package metadata

generated: components: sinks: gcp_stackdriver_metrics: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	api_key: {
		description: """
			An [API key][gcp_api_key].

			Either an API key or a path to a service account credentials JSON file can be specified.

			If both are unset, the `GOOGLE_APPLICATION_CREDENTIALS` environment variable is checked for a filename. If no
			filename is named, an attempt is made to fetch an instance service account for the compute instance the program is
			running on. If this is not on a GCE instance, then you must define it with an API key or service account
			credentials JSON file.

			[gcp_api_key]: https://cloud.google.com/docs/authentication/api-keys
			"""
		required: false
		type: string: {}
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["derived::6f45f8b8572be16d3c7f3049"]
	}
	credentials_path: {
		description: """
			Path to a [service account][gcp_service_account_credentials] credentials JSON file.

			Either an API key or a path to a service account credentials JSON file can be specified.

			If both are unset, the `GOOGLE_APPLICATION_CREDENTIALS` environment variable is checked for a filename. If no
			filename is named, an attempt is made to fetch an instance service account for the compute instance the program is
			running on. If this is not on a GCE instance, then you must define it with an API key or service account
			credentials JSON file.

			[gcp_service_account_credentials]: https://cloud.google.com/docs/authentication/production#manually
			"""
		required: false
		type: string: {}
	}
	default_namespace: {
		description: """
			The default namespace to use for metrics that do not have one.

			Metrics with the same name can only be differentiated by their namespace, and not all
			metrics have their own namespace.
			"""
		required: false
		type: string: default: "namespace"
	}
	project_id: {
		description: """
			The project ID to which to publish metrics.

			See the [Google Cloud Platform project management documentation][project_docs] for more details.

			[project_docs]: https://cloud.google.com/resource-manager/docs/creating-managing-projects
			"""
		required: true
		type: string: {}
	}
	request: {
		description: """
			Middleware settings for outbound requests.

			Various settings can be configured, such as concurrency and rate limits, timeouts, and retry behavior.

			Note that the retry backoff policy follows the Fibonacci sequence.
			"""
		required: false
		type:     _schemaDefinitions["derived::vector::sinks::util::service::TowerRequestConfig::c01c924c3c7445635044bd00"]
	}
	resource: {
		description: """
			A monitored resource.

			The monitored resource to associate the metrics with.
			"""
		required: true
		type: object: {
			examples: [{
				instanceId: "Twilight"
				projectId:  "vector-123456"
				type:       "global"
				zone:       "us-central1-a"
			}]
			options: {
				"*": {
					description: """
						Values for all of the labels listed in the associated monitored resource descriptor.

						For example, Compute Engine VM instances use the labels `projectId`, `instanceId`, and `zone`.
						"""
					required: true
					type: string: {}
				}
				type: {
					description: """
						The monitored resource type.

						For example, the type of a Compute Engine VM instance is `gce_instance`.
						"""
					required: true
					type: string: examples: ["global", "gce_instance"]
				}
			}
		}
	}
	retry_strategy: {
		description: """
			Configurable retry strategy for `http` based sinks.

			For more information about error responses, see [Client Error Responses][error_responses].

			[error_responses]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status#client_error_responses
			"""
		required: false
		type:     _schemaDefinitions["derived::vector::sinks::util::http::RetryStrategy::64fe77681a1c274eed24cca6"]
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
