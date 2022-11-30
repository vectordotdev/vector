package metadata

components: sinks: [Name=string]: {
	kind: "sink"

	features: _

	configuration: {
		inputs: base.components.sinks.configuration.inputs
		buffer: base.components.sinks.configuration.buffer
		healthcheck: {
			description: base.components.sinks.configuration.healthcheck.description
			required: base.components.sinks.configuration.healthcheck.required
			type: object: options: {
				enabled: base.components.sinks.configuration.healthcheck.type.object.options.enabled

				if features.healthcheck != _|_ {
					if features.healthcheck.uses_uri {
						uri: base.components.sinks.configuration.healthcheck.type.object.options.uri
					}
				}
			}
		}

		if features.send != _|_ {
			if features.send.proxy != _|_ {
				if features.send.proxy.enabled {
					proxy: base.components.sinks.configuration.proxy
				}
			}
		}
	}

	how_it_works: {
		if features.buffer.enabled {
			if features.send != _|_ {
				if features.send.batch != _|_ {
					if features.send.batch.enabled {
						buffers_batches: {
							title: "Buffers and batches"
							svg:   "/img/buffers-and-batches-serial.svg"
							body: #"""
								This component buffers & batches data as shown in the diagram above. You'll notice that
								Vector treats these concepts differently, instead of treating them as global concepts,
								Vector treats them as sink specific concepts. This isolates sinks, ensuring services
								disruptions are contained and delivery guarantees are honored.

								*Batches* are flushed when 1 of 2 conditions are met:

								1. The batch age meets or exceeds the configured `timeout_secs`.
								2. The batch size meets or exceeds the configured `max_bytes` or `max_events`.

								*Buffers* are controlled via the [`buffer.*`](#buffer) options.
								"""#
						}
					}
				}
			}

			if features.send == _|_ {
				buffers: {
					title: "Buffers"
					svg:   "/img/buffers.svg"
					body: """
						This component buffers events as shown in
						the diagram above. This helps to smooth out data processing if the downstream
						service applies backpressure. Buffers are controlled via the
						[`buffer.*`](#buffer) options.
						"""
				}
			}
		}

		if features.healthcheck.enabled {
			healthchecks: {
				title: "Health checks"
				body: """
					Health checks ensure that the downstream service is
					accessible and ready to accept data. This check is performed
					upon sink initialization. If the health check fails an error
					will be logged and Vector will proceed to start.
					"""
				sub_sections: [
					{
						title: "Require health checks"
						body: """
							If you'd like to exit immediately upon a health check failure, you can pass the
							`--require-healthy` flag:

							```bash
							vector --config /etc/vector/vector.toml --require-healthy
							```
							"""
					},
					{
						title: "Disable health checks"
						body: """
							If you'd like to disable health checks for this sink you can set the `healthcheck` option to
							`false`.
							"""
					},
				]
			}
		}

		if features.send != _|_ {
			if features.send.request.enabled {
				partitioning: _ | *{
					title: "Partitioning"
					body: """
						Vector supports dynamic configuration values through a simple
						template syntax. If an option supports templating, it will be
						noted with a badge and you can use event fields to create dynamic
						values. For example:

						```toml title="vector.toml"
						[sinks.my-sink]
						dynamic_option = "application={{ application_id }}"
						```

						In the above example, the `application_id` for each event will be
						used to partition outgoing data.
						"""
				}
			}
		}

		if features.send != _|_ {
			if features.send.request.enabled {
				rate_limits: {
					title: "Rate limits & adaptive concurrency"
					body:  null
					sub_sections: [
						{
							title: "Adaptive Request Concurrency (ARC)"
							body:  """
								Adaptive Request Concurrency is a feature of Vector that does away with static
								concurrency limits and automatically optimizes HTTP concurrency based on downstream
								service responses. The underlying mechanism is a feedback loop inspired by TCP
								congestion control algorithms. Checkout the [announcement blog
								post](\(urls.adaptive_request_concurrency_post)),

								We highly recommend enabling this feature as it improves
								performance and reliability of Vector and the systems it
								communicates with. As such, we have made it the default,
								and no further configuration is required.
								"""
						},
						{
							title: "Static concurrency"
							body: """
								If Adaptive Request Concurrency is not for you, you can manually set static concurrency
								limits by specifying an integer for `request.concurrency`:

								```toml title="vector.toml"
								[sinks.my-sink]
								  request.concurrency = 10
								```
								"""
						},
						{
							title: "Rate limits"
							body: """
								In addition to limiting request concurrency, you can also limit the overall request
								throughput via the `request.rate_limit_duration_secs` and `request.rate_limit_num`
								options.

								```toml title="vector.toml"
								[sinks.my-sink]
								  request.rate_limit_duration_secs = 1
								  request.rate_limit_num = 10
								```

								These will apply to both `adaptive` and fixed `request.concurrency` values.
								"""
						},
					]
				}

				retry_policy: {
					title: "Retry policy"
					body: """
						Vector will retry failed requests (status == 429, >= 500, and != 501).
						Other responses will not be retried. You can control the number of
						retry attempts and backoff rate with the `request.retry_attempts` and
						`request.retry_backoff_secs` options.
						"""
				}
			}
		}

		if features.send != _|_ {
			if features.send.tls.enabled {
				transport_layer_security: {
					title: "Transport Layer Security (TLS)"
					body:  """
						Vector uses [OpenSSL](\(urls.openssl)) for TLS protocols due to OpenSSL's maturity. You can
						enable and adjust TLS behavior using the [`tls.*`](#tls) options.
						"""
				}
			}
		}
	}

	telemetry: metrics: {
		component_received_events_count:      components.sources.internal_metrics.output.metrics.component_received_events_count
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		events_in_total:                      components.sources.internal_metrics.output.metrics.events_in_total
		utilization:                          components.sources.internal_metrics.output.metrics.utilization
		buffer_byte_size:                     components.sources.internal_metrics.output.metrics.buffer_byte_size
		buffer_events:                        components.sources.internal_metrics.output.metrics.buffer_events
		buffer_received_events_total:         components.sources.internal_metrics.output.metrics.buffer_received_events_total
		buffer_received_event_bytes_total:    components.sources.internal_metrics.output.metrics.buffer_received_event_bytes_total
		buffer_sent_events_total:             components.sources.internal_metrics.output.metrics.buffer_sent_events_total
		buffer_sent_event_bytes_total:        components.sources.internal_metrics.output.metrics.buffer_sent_event_bytes_total
		buffer_discarded_events_total:        components.sources.internal_metrics.output.metrics.buffer_discarded_events_total
	}
}
