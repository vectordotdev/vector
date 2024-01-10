package metadata

components: sinks: elasticsearch: {
	title: "Elasticsearch"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["AWS", "Azure", "Elastic", "GCP"]
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
				service: services.elasticsearch

				interface: {
					socket: {
						api: {
							title: "Elasticsearch bulk API"
							url:   urls.elasticsearch_bulk
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
				Elasticsearch's Data streams feature requires Vector to be configured with the `create` `bulk.action`.
				This is *not* enabled by default.
				"""#,
		]
		warnings: []
		notices: []
	}

	configuration: base.components.sinks.elasticsearch.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		conflicts: {
			title: "Conflicts"
			body:  """
				Vector [batches](#buffers-and-batches) data and flushes it to Elasticsearch's
				[`_bulk` API endpoint](\(urls.elasticsearch_bulk)). By default, all events are
				inserted via the `index` action, which replaces documents if an existing
				one has the same `id`. If `bulk.action` is configured with `create`, Elasticsearch
				does _not_ replace an existing document and instead returns a conflict error.
				"""
		}

		data_streams: {
			title: "Data streams"
			body:  """
				By default, Vector uses the `index` action with Elasticsearch's Bulk API.
				To use [Data streams](\(urls.elasticsearch_data_streams)), set the `mode` to
				`data_stream`. Use the combination of `data_stream.type`, `data_stream.dataset` and
				`data_stream.namespace` instead of `index`.
				"""
		}

		distribution: {
			title: "Distribution"
			body: """
				If multiple endpoints are specified in `endpoints` option, events will be distributed among them
				according to their estimated load with failover.

				Rate limit is applied to the sink as a whole, while concurrency settings manage each endpoint individually.

				Health of endpoints is actively monitored and if an endpoint is deemed unhealthy, Vector will stop sending events to it
				until it is healthy again. This is managed by a circuit breaker that monitors responses and triggers after a sufficient
				streak of failures. Once triggered it will enter exponential backoff loop and pass a single request in each iteration
				to test the endpoint. Once a successful response is received, the circuit breaker will reset.
				"""
		}

		partial_failures: {
			title: "Partial Failures"
			body:  """
				By default, Elasticsearch allows partial bulk ingestion failures. This is typically
				due to Elasticsearch index mapping errors, where data keys aren't consistently
				typed. To change this behavior, refer to the Elasticsearch [`ignore_malformed`
				setting](\(urls.elasticsearch_ignore_malformed)).

				By default, partial failures are not retried. To enable retries, set `request_retry_partial`. Once enabled it will
				retry whole partially failed requests. As such it is advised to use `id_key` to avoid duplicates.
				"""
		}

		aws_authentication: components._aws.how_it_works.aws_authentication
	}
}
