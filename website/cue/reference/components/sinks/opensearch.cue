package metadata

components: sinks: opensearch: {
	title: "OpenSearch"

	classes: {
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["AWS"]
		stateful: false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
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
			proxy: enabled: true
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
				service: services.opensearch

				interface: {
					socket: {
						api: {
							title: "OpenSearch bulk API"
							url:   "https://docs.opensearch.org/latest/api-reference/document-apis/bulk/"
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
		requirements: []
		warnings: []
	}

	configuration: generated.components.sinks.opensearch.configuration
	input:         components.sinks.elasticsearch.input
	telemetry:     components.sinks.elasticsearch.telemetry

	how_it_works: {
		configuration: {
			title: "Configuration"
			body: """
				The OpenSearch sink supports self-managed OpenSearch, Amazon OpenSearch Service,
				and Amazon OpenSearch Serverless. It shares configuration options and delivery
				behavior with the [Elasticsearch sink](../elasticsearch/).
				Existing OpenSearch configurations can switch from `type: elasticsearch` to
				`type: opensearch` without changing the other options.

				For self-managed OpenSearch, the default `api_version: auto` selects the
				compatible bulk API version:

				```yaml
				sinks:
				  opensearch:
				    type: opensearch
				    inputs: [my_source]
				    endpoints: ["https://opensearch.example.com:9200"]
				    bulk:
				      index: "logs-%Y.%m.%d"
				    auth:
				      strategy: basic
				      user: vector
				      password: "${OPENSEARCH_PASSWORD}"
				```

				For Amazon OpenSearch Service, use `auth.strategy: aws` and set `aws.region`.
				Keep `opensearch_service_type: managed`, which is the default.
				For an Amazon OpenSearch Serverless collection, also set
				`opensearch_service_type: serverless` and keep `api_version: auto`.
				Healthchecks are skipped for serverless collections.
				"""
		}
		data_streams: {
			title: "Data streams"
			body: """
				Set `mode: data_stream` to write to OpenSearch data streams. Configure a matching
				index template in OpenSearch first. Vector uses the bulk API's `create` action
				and adds the data stream timestamp and routing fields.
				"""
		}
	}
}
