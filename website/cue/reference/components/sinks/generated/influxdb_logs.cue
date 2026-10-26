package metadata

generated: components: sinks: influxdb_logs: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["bool_or_struct::vector_core::config::AcknowledgementsConfig"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::splunk_hec::common::util::SplunkHecDefaultBatchSettings>"]
	}
	bucket: {
		description:   "The name of the bucket to write into."
		relevant_when: "version = \"2\""
		required:      true
		type: string: examples: ["vector-bucket", "4d2225e4d3d49f75"]
	}
	consistency: {
		description:   "The consistency level to use for writes."
		relevant_when: "version = \"1\""
		required:      false
		type: string: examples: ["any", "one", "quorum", "all"]
	}
	database: {
		description:   "The name of the database to write into."
		relevant_when: "version = \"1\""
		required:      true
		type: string: examples: ["vector-database", "iot-store"]
	}
	encoding: {
		description: "Transformations to prepare an event for serialization."
		required:    false
		type:        _schemaDefinitions["codecs::encoding::transformer::Transformer"]
	}
	endpoint: {
		description: """
			The endpoint to send data to.

			This should be a full HTTP URI, including the scheme, host, and port.
			"""
		required: true
		type: string: examples: ["http://localhost:8086"]
	}
	host_key: {
		description: """
			Use this option to customize the key containing the hostname.

			The setting of `log_schema.host_key`, usually `host`, is used here by default.
			"""
		required: false
		type: string: examples: ["hostname"]
	}
	measurement: {
		description: "The name of the InfluxDB measurement that is written to."
		required:    true
		type: string: examples: ["vector-logs"]
	}
	message_key: {
		description: """
			Use this option to customize the key containing the message.

			The setting of `log_schema.message_key`, usually `message`, is used here by default.
			"""
		required: false
		type: string: examples: [
			"text"
		]
	}
	org: {
		description:   "The name of the organization to write into."
		relevant_when: "version = \"2\""
		required:      true
		type: string: examples: ["my-org", "33f2cff0a28e5b63"]
	}
	password: {
		description:   "The password to authenticate with."
		relevant_when: "version = \"1\""
		required:      false
		type: string: examples: ["${INFLUXDB_PASSWORD}", "influxdb4ever"]
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
	retention_policy_name: {
		description:   "The target retention policy for writes."
		relevant_when: "version = \"1\""
		required:      false
		type: string: examples: ["autogen", "one_day_only"]
	}
	source_type_key: {
		description: """
			Use this option to customize the key containing the `source_type`.

			The setting of `log_schema.source_type_key`, usually `source_type`, is used here by default.
			"""
		required: false
		type: string: examples: [
			"source"
		]
	}
	tags: {
		description: """
			The list of names of log fields that should be added as tags to each measurement.

			By default Vector adds `metric_type` as well as the configured `log_schema.host_key` and
			`log_schema.source_type_key` options.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["field1", "parent.child_field"]
		}
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
	token: {
		description: """
			The [token][token_docs] to authenticate with.

			[token_docs]: https://v2.docs.influxdata.com/v2.0/security/tokens/
			"""
		relevant_when: "version = \"2\""
		required:      true
		type: string: examples: ["${INFLUXDB_TOKEN}", "ef8d5de700e7989468166c40fc8a0ccd"]
	}
	username: {
		description:   "The username to authenticate with."
		relevant_when: "version = \"1\""
		required:      false
		type: string: examples: ["todd", "vector-source"]
	}
	version: {
		description: "The InfluxDB API version to use."
		required:    true
		type: string: enum: {
			"1": "InfluxDB v0.x/v1.x."
			"2": "InfluxDB v2.x."
		}
	}
}
