package metadata

components: sources: splunk_hec: {
	_port: 8080

	title: "Splunk HTTP Event Collector (HEC)"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["aggregator"]
		development:   "stable"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		multiline: enabled: false
		receive: {
			from: {
				service: services.splunk

				interface: socket: {
					api: {
						title: "Splunk HEC"
						url:   urls.splunk_hec_protocol
					}
					direction: "incoming"
					port:      _port
					protocols: ["http"]
					ssl: "optional"
				}
			}

			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				enabled_default:        false
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		acknowledgements: configuration._acknowledgements & {
			type: object: {
				options: {
					max_number_of_ack_channels: {
						common:      false
						description: "The maximum number of Splunk HEC channels clients can use with this source. Minimum of `1`."
						required:    false
						type: uint: {
							default: 1000000
							unit:    null
						}
					}
					max_pending_acks: {
						common:      false
						description: "The maximum number of ack statuses pending query across all channels. Equivalent to the `max_number_of_acked_requests_pending_query` Splunk HEC setting. Minimum of `1`."
						required:    false
						type: uint: {
							default: 10000000
							unit:    null
						}
					}
					max_pending_acks_per_channel: {
						common:      false
						description: "The maximum number of ack statuses pending query for a single channel. Equivalent to the `max_number_of_acked_requests_pending_query_per_ack_channel` Splunk HEC setting. Minimum of `1`."
						required:    false
						type: uint: {
							default: 1000000
							unit:    null
						}
					}
					ack_idle_cleanup: {
						common:      false
						description: "Whether or not to remove channels after idling for `max_idle_time` seconds. A channel is idling if it is not used for sending data or querying ack statuses."
						required:    false
						type: bool: {
							default: false
						}
					}
					max_idle_time: {
						common:      false
						description: "The amount of time a channel is allowed to idle before removal. Channels can potentially idle for longer than this setting but clients should not rely on such behavior. Minimum of `1`."
						required:    false
						type: uint: {
							default: 300
							unit:    "seconds"
						}
					}
				}
			}
		}
		address: {
			common:      true
			description: "The address to accept connections on."
			required:    false
			type: string: {
				default: "0.0.0.0:\(_port)"
			}
		}
		token: {
			common:      true
			description: "If supplied, incoming requests must supply this token in the `Authorization` header, just as a client would if it was communicating with the Splunk HEC endpoint directly. If _not_ supplied, the `Authorization` header will be ignored and requests will not be authenticated."
			required:    false
			warnings: ["This option has been deprecated, the `valid_tokens` option should be used."]
			type: string: {
				default: null
				examples: ["A94A8FE5CCB19BA61C4C08"]
			}
		}
		valid_tokens: {
			common:      true
			description: "If supplied, incoming requests must supply one of these tokens in the `Authorization` header, just as a client would if it was communicating with the Splunk HEC endpoint directly. If _not_ supplied, the `Authorization` header will be ignored and requests will not be authenticated."
			required:    false
			type: array: {
				default: null

				items: type: string: {
					examples: ["A94A8FE5CCB19BA61C4C08"]
				}
			}
		}
		store_hec_token: {
			common:      false
			description: "When incoming requests contain a Splunk HEC token, if this setting is set to `true`, the token will kept in the event metadata and will be used if the event is sent to a Splunk HEC sink."
			required:    false
			type: bool: default: false
		}
	}

	output: logs: event: {
		description: "A single event"
		fields: {
			message: fields._raw_line
			splunk_channel: {
				description: "The Splunk channel, value of the `X-Splunk-Request-Channel` header or `channel` query parameter, in that order of precedence."
				required:    true
				type: timestamp: {}
			}
			timestamp: fields._current_timestamp
		}
	}

	telemetry: metrics: {
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		events_in_total:                      components.sources.internal_metrics.output.metrics.events_in_total
		http_request_errors_total:            components.sources.internal_metrics.output.metrics.http_request_errors_total
		requests_received_total:              components.sources.internal_metrics.output.metrics.requests_received_total
	}

	how_it_works: {
		indexer_acknowledgements: {
			title: "Indexer Acknowledgements"
			body: """
				With acknowledgements enabled, the source uses the [Splunk HEC indexer acknowledgements protocol](https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/AboutHECIDXAck) to allow clients to verify data has been delivered to destination sinks.
				To summarize the protocol, each request to the source is associated with an integer identifier (an ack id) that the client is given and can use to query for the status of the request.
				"""
		}
	}
}
