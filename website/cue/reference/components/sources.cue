package metadata

components: sources: [Name=string]: {
	kind: "source"

	features: _

	configuration: base.components.sources.configuration

	output: {
		logs?: [Name=string]: {
			fields: {
				_current_timestamp: {
					description: string | *"The exact time the event was ingested into Vector."
					required:    true
					type: timestamp: {}
				}

				_local_host: {
					description: string | *"The local hostname, equivalent to the `gethostname` command."
					required:    true
					type: string: {
						examples: [_values.local_host]
					}
				}

				_raw_line: {
					description: "The raw line, unparsed."
					required:    true
					type: string: {
						examples: ["2019-02-13T19:48:34+00:00 [info] Started GET \"/\" for 127.0.0.1"]
					}
				}

				_client_metadata: {
					common:      false
					description: "Client TLS metadata."
					required:    false
					type: object: {
						options: {
							subject: {
								common:      true
								description: "The subject from the client TLS certificate. Only added if `tls.client_metadata_key` is set. Key name depends on configured `client_metadata_key`"
								required:    false
								type: string: {
									default: null
									examples: [ "CN=localhost,OU=Vector,O=Datadog,L=New York,ST=New York,C=US"]
								}
							}
						}
					}
				}
			}
		}
	}

	how_it_works: {
		_tls: {
			title: "Transport Layer Security (TLS)"
			body:  """
				  Vector uses [OpenSSL](\(urls.openssl)) for TLS protocols. You can
				  adjust TLS behavior via the `tls.*` options.
				  """
		}

		if features.collect != _|_ {
			if features.collect.checkpoint.enabled {
				checkpointing: {
					title: "Checkpointing"
					body: """
						Vector checkpoints the current read position after each
						successful read. This ensures that Vector resumes where it left
						off if restarted, preventing data from being read twice. The
						checkpoint positions are stored in the data directory which is
						specified via the global `data_dir` option, but can be overridden
						via the `data_dir` option in the file source directly.
						"""
				}
			}
		}

		context: {
			title: "Context"
			body:  """
				By default, the `\( Name )` source augments events with helpful
				context keys.
				"""
		}

		if features.collect != _|_ {
			if features.collect.tls != _|_ {
				if features.collect.tls.enabled {
					tls: _tls
				}
			}
		}

		if features.receive != _|_ {
			if features.receive.tls.enabled {
				tls: _tls
			}
		}
	}

	telemetry: metrics: {
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		source_lag_time_seconds:          components.sources.internal_metrics.output.metrics.source_lag_time_seconds
	}
}
