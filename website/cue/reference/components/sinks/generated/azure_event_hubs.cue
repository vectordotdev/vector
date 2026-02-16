package metadata

generated: components: sinks: azure_event_hubs: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type: object: options: enabled: {
			description: """
				Controls whether or not end-to-end acknowledgements are enabled.

				When enabled for a sink, any source that supports end-to-end
				acknowledgements that is connected to that sink waits for events
				to be acknowledged by **all connected sinks** before acknowledging them at the source.

				Enabling or disabling acknowledgements at the sink level takes precedence over any global
				[`acknowledgements`][global_acks] configuration.

				[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
				"""
			required: false
			type: bool: {}
		}
	}
	connection_string: {
		description: """
			The connection string for the Event Hubs namespace.

			If not set, authentication falls back to `azure_identity` (e.g., Managed Identity).
			In that case, `namespace` and `event_hub_name` must be provided.
			"""
		required: false
		type: string: {
			examples: ["Endpoint=sb://mynamespace.servicebus.windows.net/;SharedAccessKeyName=mykeyname;SharedAccessKey=mykey;EntityPath=my-event-hub"]
		}
	}
	namespace: {
		description: """
			The fully qualified Event Hubs namespace host.

			Required when not using a connection string.
			"""
		required: false
		type: string: {
			examples: ["mynamespace.servicebus.windows.net"]
		}
	}
	event_hub_name: {
		description: """
			The name of the Event Hub to send events to.
			"""
		required: false
		type: string: {
			examples: ["my-event-hub"]
		}
	}
	partition_id_field: {
		description: """
			The log field to use as the Event Hubs partition ID.

			If set, events are routed to the specified partition. If not set,
			Event Hubs automatically selects a partition (round-robin).
			"""
		required: false
		type: string: {
			examples: [".partition_id", ".metadata.partition"]
		}
	}
	encoding: {
		description: "Encoding configuration."
		required:    true
		type: object: options: codec: {
			description: "The encoding codec to use."
			required:    true
			type: string: {
				enum: {
					json: "JSON encoding."
					text: "Plain text encoding."
				}
			}
		}
	}
	request: {
		description: "Tower request configuration for controlling concurrency and rate limiting."
		required:    false
		type: object: options: {
			concurrency: {
				description: "The maximum number of in-flight requests."
				required:    false
				type: uint: {
					default: 5
					examples: [5, 10, 20]
					unit: "requests"
				}
			}
			rate_limit_duration_secs: {
				description: "The time window for the rate limiter."
				required:    false
				type: uint: {
					default: 1
					unit:    "seconds"
				}
			}
			rate_limit_num: {
				description: "The maximum number of requests in the rate limit window."
				required:    false
				type: uint: {
					default: 100
					unit:    "requests"
				}
			}
			timeout_secs: {
				description: "The maximum time a request can take before being aborted."
				required:    false
				type: uint: {
					default: 60
					unit:    "seconds"
				}
			}
		}
	}
}
