package metadata

generated: components: sources: gcp_pubsub: configuration: {
	ack_deadline_seconds: {
		deprecated:         true
		deprecated_message: "This option has been deprecated, use `ack_deadline_secs` instead."
		description: """
			The acknowledgement deadline, in seconds, to use for this stream.

			Messages that are not acknowledged when this deadline expires may be retransmitted.
			"""
		required: false
		type: uint: {}
	}
	ack_deadline_secs: {
		description: """
			The acknowledgement deadline, in seconds, to use for this stream.

			Messages that are not acknowledged when this deadline expires may be retransmitted.
			"""
		required: false
		type: uint: {
			default: 600
			unit:    "seconds"
		}
	}
	acknowledgements: {
		deprecated: true
		description: """
			Controls how acknowledgements are handled by this source.

			This setting is **deprecated** in favor of enabling `acknowledgements` at the [global][global_acks] or sink level.

			Enabling or disabling acknowledgements at the source level has **no effect** on acknowledgement behavior.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::SourceAcknowledgementsConfig"]
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
	decoding: {
		description: """
			Configures how events are decoded from raw bytes. Note some decoders can also determine the event output
			type (log, metric, trace).
			"""
		required: false
		type:     _schemaDefinitions["derived::2c0db0ef1f05c78303bd4391"]
	}
	endpoint: {
		description: "The endpoint from which to pull data."
		required:    false
		type: string: {
			default: "https://pubsub.googleapis.com"
			examples: ["https://us-central1-pubsub.googleapis.com"]
		}
	}
	framing: {
		description: """
			Framing configuration.

			Framing handles how events are separated when encoded in a raw byte form, where each event is
			a frame that must be prefixed, or delimited, in a way that marks where an event begins and
			ends within the byte stream.
			"""
		required: false
		type:     _schemaDefinitions["derived::2a4f9b813a8f495175f80ccf"]
	}
	full_response_size: {
		description: """
			The number of messages in a response to mark a stream as
			"busy". This is used to determine if more streams should be
			started.

			The GCP Pub/Sub servers send responses with 100 or more messages when
			the subscription is busy.
			"""
		required: false
		type: uint: default: 100
	}
	keepalive_secs: {
		description: """
			The amount of time, in seconds, with no received activity
			before sending a keepalive request. If this is set larger than
			`60`, you may see periodic errors sent from the server.
			"""
		required: false
		type: float: {
			default: 60.0
			unit:    "seconds"
		}
	}
	max_concurrency: {
		description: "The maximum number of concurrent stream connections to open at once."
		required:    false
		type: uint: default: 10
	}
	poll_time_seconds: {
		description: """
			How often to poll the currently active streams to see if they
			are all busy and so open a new stream.
			"""
		required: false
		type: float: {
			default: 2.0
			unit:    "seconds"
		}
	}
	project: {
		description: "The project name from which to pull logs."
		required:    true
		type: string: examples: ["my-log-source-project"]
	}
	retry_delay_seconds: {
		deprecated:         true
		deprecated_message: "This option has been deprecated, use `retry_delay_secs` instead."
		description:        "The amount of time, in seconds, to wait between retry attempts after an error."
		required:           false
		type: float: {}
	}
	retry_delay_secs: {
		description: "The amount of time, in seconds, to wait between retry attempts after an error."
		required:    false
		type: float: {
			default: 1.0
			unit:    "seconds"
		}
	}
	subscription: {
		description: "The subscription within the project which is configured to receive logs."
		required:    true
		type: string: examples: ["my-vector-source-subscription"]
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
