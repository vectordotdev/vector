package metadata

generated: components: sinks: gcp_stackdriver_logs: configuration: {
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
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::util::batch::RealtimeSizeBasedDefaultBatchSettings>"]
	}
	billing_account_id: {
		description: """
			The billing account ID to which to publish logs.

			Exactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set.
			"""
		required: true
		type: string: {}
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
	encoding: {
		description: "Transformations to prepare an event for serialization."
		required:    false
		type:        _schemaDefinitions["codecs::encoding::transformer::Transformer"]
	}
	folder_id: {
		description: """
			The folder ID to which to publish logs.

			See the [Google Cloud Platform folder documentation][folder_docs] for more details.

			Exactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set.

			[folder_docs]: https://cloud.google.com/resource-manager/docs/creating-managing-folders
			"""
		required: true
		type: string: {}
	}
	labels: {
		description: "A map of key, value pairs that provides additional information about the log entry."
		required:    false
		type: object: {
			examples: [{
				label_1: "value_1"
				label_2: "label-{{ template_value_2 }}"
			}]
			options: "*": {
				description: "A key, value pair that describes a log entry."
				required:    true
				type: string: syntax: "template"
			}
		}
	}
	labels_key: {
		description: """
			The value of this field is used to retrieve the associated labels from the `jsonPayload`
			and extract their values to set as LogEntry labels.
			"""
		required: false
		type: string: {
			default: "logging.googleapis.com/labels"
			examples: ["logging.googleapis.com/labels"]
		}
	}
	log_id: {
		description: """
			The log ID to which to publish logs.

			This is a name you create to identify this log stream.
			"""
		required: true
		type: string: syntax: "template"
	}
	organization_id: {
		description: """
			The organization ID to which to publish logs.

			This would be the identifier assigned to your organization on Google Cloud Platform.

			Exactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set.
			"""
		required: true
		type: string: {}
	}
	project_id: {
		description: """
			The project ID to which to publish logs.

			See the [Google Cloud Platform project management documentation][project_docs] for more details.

			Exactly one of `billing_account_id`, `folder_id`, `organization_id`, or `project_id` must be set.

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
		type:     _schemaDefinitions["derived::c01c924c3c7445635044bd00"]
	}
	resource: {
		description: """
			A monitored resource.

			The monitored resource to associate the logs with.
			"""
		required: true
		type: object: {
			examples: [{
				instanceId: "Twilight"
				zone:       "zone-{{ zone }}"
			}]
			options: {
				"*": {
					description: "A type-specific label."
					required:    true
					type: string: syntax: "template"
				}
				type: {
					description: """
						The monitored resource type.

						For example, the type of a Compute Engine VM instance is `gce_instance`.
						See the [Google Cloud Platform monitored resource documentation][gcp_resources] for
						more details.

						[gcp_resources]: https://cloud.google.com/monitoring/api/resources
						"""
					required: true
					type: string: {}
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
		type:     _schemaDefinitions["derived::64fe77681a1c274eed24cca6"]
	}
	severity_key: {
		description: """
			The field of the log event from which to take the outgoing log’s `severity` field.

			The named field is removed from the log event if present, and must be either an integer
			between 0 and 800 or a string containing one of the [severity level names][sev_names] (case
			is ignored) or a common prefix such as `err`.

			If no severity key is specified, the severity of outgoing records is set to 0 (`DEFAULT`).

			See the [GCP Stackdriver Logging LogSeverity description][logsev_docs] for more details on
			the value of the `severity` field.

			[sev_names]: https://cloud.google.com/logging/docs/reference/v2/rest/v2/LogEntry#logseverity
			[logsev_docs]: https://cloud.google.com/logging/docs/reference/v2/rest/v2/LogEntry#logseverity
			"""
		required: false
		type: string: examples: ["severity"]
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
