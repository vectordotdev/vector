package metadata

generated: components: sinks: influxdb_metrics: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type:        _schemaDefinitions["vector::sinks::util::batch::BatchConfig<vector::sinks::greptimedb::GreptimeDBDefaultBatchSettings>"]
	}
	bucket: {
		description: """
			The name of the bucket to write into.

			Only relevant when using InfluxDB v2.x and above.
			"""
		minimal:       true
		relevant_when: "version = \"2\""
		required:      false
		required_when: "version = \"2\""
		type: string: examples: ["vector-bucket"]
	}
	consistency: {
		description: """
			The consistency level to use for writes.

			Only relevant when using InfluxDB v0.x/v1.x.
			"""
		relevant_when: "version = \"1\""
		required:      false
		type: string: examples: [
			"any"
		]
	}
	database: {
		description: """
			The name of the database to write into.

			Only relevant when using InfluxDB v0.x/v1.x.
			"""
		relevant_when: "version = \"1\""
		required:      false
		required_when: "version = \"1\""
		type: string: examples: ["vector-database"]
	}
	default_namespace: {
		description: """
			Sets the default namespace for any metrics sent.

			This namespace is only used if a metric has no existing namespace. When a namespace is
			present, it is used as a prefix to the metric name, and separated with a period (`.`).
			"""
		required: false
		type: string: examples: ["service"]
	}
	endpoint: {
		description: """
			The endpoint to send data to.

			This should be a full HTTP URI, including the scheme, host, and port.
			"""
		required: true
		type: string: examples: ["http://localhost:8086/"]
	}
	org: {
		description: """
			The name of the organization to write into.

			Only relevant when using InfluxDB v2.x and above.
			"""
		minimal:       true
		relevant_when: "version = \"2\""
		required:      false
		required_when: "version = \"2\""
		type: string: examples: [
			"my-org"
		]
	}
	password: {
		description: """
			The password to authenticate with.

			Only relevant when using InfluxDB v0.x/v1.x.
			"""
		relevant_when: "version = \"1\""
		required:      false
		type: string: examples: ["${INFLUXDB_PASSWORD}"]
	}
	quantiles: {
		description: "The list of quantiles to calculate when sending distribution metrics."
		required:    false
		type: array: {
			default: [0.5, 0.75, 0.9, 0.95, 0.99]
			items: type: float: {}
		}
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
		description: """
			The target retention policy for writes.

			Only relevant when using InfluxDB v0.x/v1.x.
			"""
		relevant_when: "version = \"1\""
		required:      false
		type: string: examples: ["autogen"]
	}
	tags: {
		description: "A map of additional tags, in the key/value pair format, to add to each measurement."
		required:    false
		type: object: {
			examples: [{
				region: "us-west-1"
			}]
			options: "*": {
				description: "A tag key/value pair."
				required:    true
				type: string: {}
			}
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

			Only relevant when using InfluxDB v2.x and above.

			[token_docs]: https://v2.docs.influxdata.com/v2.0/security/tokens/
			"""
		minimal:       true
		relevant_when: "version = \"2\""
		required:      false
		required_when: "version = \"2\""
		type: string: examples: ["${INFLUXDB_TOKEN}"]
	}
	username: {
		description: """
			The username to authenticate with.

			Only relevant when using InfluxDB v0.x/v1.x.
			"""
		relevant_when: "version = \"1\""
		required:      false
		type: string: examples: [
			"todd"
		]
	}
	version: {
		description: """
			The InfluxDB API version to use.

			Omitting this option is deprecated and it will be required in a future release. When
			unset, the version is temporarily inferred from the configured settings.
			"""
		minimal:  true
		required: false
		type: string: {
			enum: {
				"1": "InfluxDB v0.x/v1.x."
				"2": "InfluxDB v2.x."
			}
			examples: ["2", "1"]
		}
	}
}
