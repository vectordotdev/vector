package metadata

components: sources: internal_metrics: {
	title: "Internal Metrics"

	description: """
		Exposes Vector's own internal metrics, allowing you to collect, process,
		and route Vector's internal metrics just like other metrics.
		"""

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		deployment_roles: ["aggregator", "daemon", "sidecar"]
		development:   "stable"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		acknowledgements: false
		multiline: enabled: false
	}

	support: {
		notices: []
		requirements: []
		warnings: []
	}

	installation: {
		platform_name: null
	}

	configuration: generated.components.sources.internal_metrics.configuration

	output: metrics: {
		// Default internal metrics tags
		_internal_metrics_tags: {
			pid: {
				description: "The process ID of the Vector instance."
				required:    false
				examples: ["4232"]
			}
			host: {
				description: "The hostname of the system Vector is running on."
				required:    false
				examples: [_values.local_host]
			}
		}

		// Helpful tag groupings
		_component_tags: _internal_metrics_tags & {
			component_kind: _component_kind
			component_id:   _component_id
			component_type: _component_type
		}

		// All available tags
		_collector: {
			description: "Which collector this metric comes from."
			required:    true
		}
		_component_kind: {
			description: "The Vector component kind."
			required:    true
			enum: {
				"sink":      "Vector sink components"
				"source":    "Vector source components"
				"transform": "Vector transform components"
			}
		}
		_component_id: {
			description: "The Vector component ID."
			required:    true
			examples: ["my_source", "my_sink"]
		}
		_component_type: {
			description: "The Vector component type."
			required:    true
			examples: ["file", "http", "honeycomb", "splunk_hec"]
		}
		_endpoint: {
			description: "The absolute path of originating file."
			required:    true
			examples: ["http://localhost:8080/server-status?auto"]
		}
		_error_type: {
			description: "The type of the error"
			required:    true
			enum: {
				"acknowledgements_failed":     "The acknowledgement operation failed."
				"delete_failed":               "The file deletion failed."
				"encode_failed":               "The encode operation failed."
				"field_missing":               "The event field was missing."
				"glob_failed":                 "The glob pattern match operation failed."
				"http_error":                  "The HTTP request resulted in an error code."
				"invalid_metric":              "The metric was invalid."
				"kafka_offset_update":         "The consumer offset update failed."
				"kafka_read":                  "The message from Kafka was invalid."
				"mapping_failed":              "The mapping failed."
				"match_failed":                "The match operation failed."
				"out_of_order":                "The event was out of order."
				"parse_failed":                "The parsing operation failed."
				"read_failed":                 "The file read operation failed."
				"render_error":                "The rendering operation failed."
				"stream_closed":               "The downstream was closed, forwarding the event(s) failed."
				"type_conversion_failed":      "The type conversion operating failed."
				"type_field_does_not_exist":   "The type field does not exist."
				"type_ip_address_parse_error": "The IP address did not parse."
				"unlabeled_event":             "The event was not labeled."
				"value_invalid":               "The value was invalid."
				"watch_failed":                "The file watch operation failed."
				"write_failed":                "The file write operation failed."
			}
		}
		_file: {
			description: "The file that produced the error"
			required:    false
		}
		_grpc_method: {
			description: "The name of the method called on the gRPC service."
			required:    true
		}
		_grpc_service: {
			description: "The gRPC service name."
			required:    true
		}
		_grpc_status: {
			description: "The human-readable [gRPC status code](\(urls.grpc_status_code))."
			required:    true
		}
		_host: {
			description: "The hostname of the originating system."
			required:    true
			examples: [_values.local_host]
		}
		_mode: {
			description: "The connection mode used by the component."
			required:    false
			enum: {
				udp:  "User Datagram Protocol"
				tcp:  "Transmission Control Protocol"
				unix: "Unix domain socket"
			}
		}
		_output: {
			description: "The specific output of the component."
			required:    false
		}
		_stage: {
			description: "The stage within the component at which the error occurred."
			required:    true
			enum: {
				receiving:  "While receiving data."
				processing: "While processing data within the component."
				sending:    "While sending data."
			}
		}
		_status: {
			description: "The HTTP status code of the request."
			required:    false
		}
		_method: {
			description: "The HTTP method of the request."
			required:    false
		}
		_path: {
			description: "The path that produced the error."
			required:    true
		}
		_reason: {
			description: "The type of the error"
			required:    true
			enum: {
				"out_of_order": "The event was out of order."
				"oversized":    "The event was too large."
			}
		}
	}

	how_it_works: {}
}
