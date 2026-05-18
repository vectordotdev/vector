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
		collect: {
			checkpoint: enabled: false
			from: service:       services.vector
		}
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

		// Instance-level "process" metrics
		active_clients: {
			tags:              _component_tags
		}
		aggregate_events_recorded_total: {
			tags:              _component_tags
		}
		aggregate_failed_updates: {
			tags:              _component_tags
		}
		aggregate_flushes_total: {
			tags:              _component_tags
		}
		api_started_total: {
			tags:              _internal_metrics_tags
		}
		component_timed_out_events_total: {
			tags:              _component_tags
		}
		component_timed_out_requests_total: {
			tags:              _component_tags
		}
		connection_established_total: {
			tags:              _internal_metrics_tags
		}
		connection_send_errors_total: {
			tags:              _internal_metrics_tags
		}
		connection_shutdown_total: {
			tags:              _internal_metrics_tags
		}
		quit_total: {
			tags:              _internal_metrics_tags
		}
		reloaded_total: {
			tags:              _internal_metrics_tags
		}
		started_total: {
			tags:              _internal_metrics_tags
		}
		stopped_total: {
			tags:              _internal_metrics_tags
		}

		// Metrics emitted by one or more components
		// Reusable metric definitions
		adaptive_concurrency_averaged_rtt: {
			tags:              _component_tags
		}
		adaptive_concurrency_in_flight: {
			tags:              _component_tags
		}
		adaptive_concurrency_limit: {
			tags:              _component_tags
		}
		adaptive_concurrency_observed_rtt: {
			tags:              _component_tags
		}
		checkpoints_total: {
			tags:              _internal_metrics_tags
		}
		checksum_errors_total: {
			tags: _internal_metrics_tags & {
				file: _file
			}
		}
		collect_completed_total: {
			tags:              _internal_metrics_tags
		}
		collect_duration_seconds: {
			tags:              _internal_metrics_tags
		}
		command_executed_total: {
			tags:              _component_tags
		}
		command_execution_duration_seconds: {
			tags:              _component_tags
		}
		connection_read_errors_total: {
			tags: _component_tags & {
				mode: {
					description: ""
					required:    true
					enum: {
						udp: "User Datagram Protocol"
					}
				}
			}
		}
		container_processed_events_total: {
			tags:              _component_tags
		}
		containers_unwatched_total: {
			tags:              _component_tags
		}
		containers_watched_total: {
			tags:              _component_tags
		}
		doris_bytes_loaded_total: {
			tags:              _component_tags
		}
		doris_rows_filtered_total: {
			tags:              _component_tags
		}
		doris_rows_loaded_total: {
			tags:              _component_tags
		}
		k8s_format_picker_edge_cases_total: {
			tags:              _component_tags
		}
		k8s_docker_format_parse_failures_total: {
			tags:              _component_tags
		}
		events_discarded_total: {
			tags: _internal_metrics_tags & {
				reason: _reason
			}
		}
		component_latency_seconds: {
			tags:              _internal_metrics_tags
		}
		component_latency_mean_seconds: {
			tags:              _internal_metrics_tags
		}
		buffer_byte_size: {
			tags:               _component_tags
		}
		buffer_events: {
			tags:               _component_tags
		}
		buffer_size_bytes: {
			tags:              _component_tags
		}
		buffer_size_events: {
			tags:              _component_tags
		}
		buffer_discarded_events_total: {
			tags:              _component_tags
		}
		buffer_received_event_bytes_total: {
			tags:              _component_tags
		}
		buffer_received_events_total: {
			tags:              _component_tags
		}
		buffer_send_duration_seconds: {
			tags:              _component_tags
		}
		buffer_sent_event_bytes_total: {
			tags:              _component_tags
		}
		buffer_sent_events_total: {
			tags:              _component_tags
		}
		component_discarded_events_total: {
			tags: _component_tags & {
				intentional: {
					description: "True if the events were discarded intentionally, like a `filter` transform, or false if due to an error."
					required:    true
				}
			}
		}
		component_errors_total: {
			tags: _component_tags & {
				error_type: _error_type
				stage:      _stage
			}
		}
		component_received_bytes_total: {
			tags:              component_received_events_total.tags
		}
		component_received_bytes: {
			tags:              component_received_events_total.tags
		}
		component_received_events_total: {
			tags: _component_tags & {
				file: {
					description: "The file from which the data originated."
					required:    false
				}
				uri: {
					description: "The sanitized URI from which the data originated."
					required:    false
				}
				container_name: {
					description: "The name of the container from which the data originated."
					required:    false
				}
				pod_name: {
					description: "The name of the pod from which the data originated."
					required:    false
				}
				peer_addr: {
					description: "The IP from which the data originated."
					required:    false
				}
				peer_path: {
					description: "The pathname from which the data originated."
					required:    false
				}
				mode: _mode
			}
		}
		component_received_events_count: {
			tags: _component_tags & {
				file: {
					description: "The file from which the data originated."
					required:    false
				}
				uri: {
					description: "The sanitized URI from which the data originated."
					required:    false
				}
				container_name: {
					description: "The name of the container from which the data originated."
					required:    false
				}
				pod_name: {
					description: "The name of the pod from which the data originated."
					required:    false
				}
				peer_addr: {
					description: "The IP from which the data originated."
					required:    false
				}
				peer_path: {
					description: "The pathname from which the data originated."
					required:    false
				}
				mode: _mode
			}
		}
		component_received_event_bytes_total: {
			tags:              component_received_events_total.tags
		}
		component_sent_bytes_total: {
			tags: _component_tags & {
				endpoint: {
					description: "The endpoint to which the bytes were sent. For HTTP, this will be the host and path only, excluding the query string."
					required:    false
				}
				file: {
					description: "The absolute path of the destination file."
					required:    false
				}
				protocol: {
					description: "The protocol used to send the bytes."
					required:    true
				}
				region: {
					description: "The AWS region name to which the bytes were sent. In some configurations, this may be a literal hostname."
					required:    false
				}
			}
		}
		component_sent_events_total: {
			tags: _component_tags & {output: _output}
		}
		component_sent_event_bytes_total: {
			tags: _component_tags & {output: _output}
		}
		internal_metrics_cardinality: {
			tags: {}
		}
		internal_metrics_cardinality_total: {
			tags:              internal_metrics_cardinality.tags
		}
		kafka_queue_messages: {
			tags:              _component_tags
		}
		kafka_queue_messages_bytes: {
			tags:              _component_tags
		}
		kafka_requests_total: {
			tags:              _component_tags
		}
		kafka_requests_bytes_total: {
			tags:              _component_tags
		}
		kafka_responses_total: {
			tags:              _component_tags
		}
		kafka_responses_bytes_total: {
			tags:              _component_tags
		}
		kafka_produced_messages_total: {
			tags:              _component_tags
		}
		kafka_produced_messages_bytes_total: {
			tags:              _component_tags
		}
		kafka_consumed_messages_total: {
			tags:              _component_tags
		}
		kafka_consumed_messages_bytes_total: {
			tags:              _component_tags
		}
		kafka_consumer_lag: {
			tags: _component_tags & {
				topic_id: {
					description: "The Kafka topic id."
					required:    true
				}
				partition_id: {
					description: "The Kafka partition id."
					required:    true
				}
			}
		}
		files_added_total: {
			tags: _internal_metrics_tags & {
				file: _file
			}
		}
		files_deleted_total: {
			tags: _internal_metrics_tags & {
				file: _file
			}
		}
		files_resumed_total: {
			tags: _internal_metrics_tags & {
				file: _file
			}
		}
		files_unwatched_total: {
			tags: _internal_metrics_tags & {
				file: _file
			}
		}
		open_files: {
			tags:              _component_tags
		}
		grpc_server_messages_received_total: {
			tags: _component_tags & {
				grpc_method:  _grpc_method
				grpc_service: _grpc_service
			}
		}
		grpc_server_messages_sent_total: {
			tags: _component_tags & {
				grpc_method:  _grpc_method
				grpc_service: _grpc_service
				grpc_status:  _grpc_status
			}
		}
		grpc_server_handler_duration_seconds: {
			tags: _component_tags & {
				grpc_method:  _grpc_method
				grpc_service: _grpc_service
				grpc_status:  _grpc_status
			}
		}
		http_client_response_rtt_seconds: {
			tags: _component_tags & {
				status: _status
			}
		}
		http_client_requests_sent_total: {
			tags: _component_tags & {
				method: _method
			}
		}
		http_client_responses_total: {
			tags: _component_tags & {
				status: _status
			}
		}
		http_client_rtt_seconds: {
			tags:              _component_tags
		}
		http_requests_total: {
			tags:              _component_tags
		}
		http_server_requests_received_total: {
			tags: _component_tags & {
				method: _method
				path:   _path
			}
		}
		http_server_responses_sent_total: {
			tags: _component_tags & {
				method: _method
				path:   _path
				status: _status
			}
		}
		http_server_handler_duration_seconds: {
			tags: _component_tags & {
				method: _method
				path:   _path
				status: _status
			}
		}
		invalid_record_total: {
			tags:              _component_tags
		}
		lua_memory_used_bytes: {
			tags:              _internal_metrics_tags
		}
		metadata_refresh_failed_total: {
			tags:              _component_tags
		}
		metadata_refresh_successful_total: {
			tags:              _component_tags
		}
		open_connections: {
			tags:              _internal_metrics_tags
		}
		protobuf_decode_errors_total: {
			tags:              _component_tags
		}
		send_errors_total: {
			tags:              _component_tags
		}
		source_lag_time_seconds: {
			tags:              _component_tags
		}
		source_send_batch_latency_seconds: {
			tags:              _component_tags
		}
		source_send_latency_seconds: {
			tags:              _component_tags
		}
		source_buffer_max_byte_size: {
			tags: _component_tags & {
				output: _output
			}
		}
		source_buffer_max_event_size: {
			tags: _component_tags & {
				output: _output
			}
		}
		source_buffer_max_size_bytes: {
			tags: _component_tags & {
				output: _output
			}
		}
		source_buffer_max_size_events: {
			tags: _component_tags & {
				output: _output
			}
		}
		source_buffer_utilization: {
			tags: _component_tags & {
				output: _output
			}
		}
		source_buffer_utilization_level: {
			tags: _component_tags & {
				output: _output
			}
		}
		source_buffer_utilization_mean: {
			tags: _component_tags & {
				output: _output
			}
		}
		splunk_pending_acks: {
			tags:              _component_tags
		}
		streams_total: {
			tags:              _component_tags
		}
		s3_object_processing_failed_duration_seconds: {
			tags: _component_tags & {
				bucket: {
					description: "The name of the S3 bucket."
					required:    true
				}
			}
		}
		s3_object_processing_succeeded_duration_seconds: {
			tags: _component_tags & {
				bucket: {
					description: "The name of the S3 bucket."
					required:    true
				}
			}
		}
		sqs_message_delete_succeeded_total: {
			tags:              _component_tags
		}
		sqs_message_processing_succeeded_total: {
			tags:              _component_tags
		}
		sqs_message_receive_succeeded_total: {
			tags:              _component_tags
		}
		sqs_message_received_messages_total: {
			tags:              _component_tags
		}
		sqs_s3_event_record_ignored_total: {

			tags: _component_tags & {
				ignore_type: {
					description: "The reason for ignoring the S3 record"
					required:    true
					enum: {
						"invalid_event_kind": "The kind of invalid event."
					}
				}
			}
		}
		stale_events_flushed_total: {
			tags:              _component_tags
		}
		stdin_reads_failed_total: {
			tags:              _component_tags
		}
		tag_value_limit_exceeded_total: {
			tags: _component_tags & {
				metric_name: {
					description: """
						The name of the metric whose tag value limit was exceeded.
						Only present when `internal_metrics.include_extended_tags` is enabled.
						"""
					required: false
				}
				tag_key: {
					description: """
						The key of the tag whose value limit was exceeded.
						Only present when `internal_metrics.include_extended_tags` is enabled.
						"""
					required: false
				}
			}
		}
		timestamp_parse_errors_total: {
			tags:              _component_tags
		}
		transform_buffer_max_byte_size: {
			tags: _component_tags & {
				output: _output
			}
		}
		transform_buffer_max_event_size: {
			tags: _component_tags & {
				output: _output
			}
		}
		transform_buffer_max_size_bytes: {
			tags: _component_tags & {
				output: _output
			}
		}
		transform_buffer_max_size_events: {
			tags: _component_tags & {
				output: _output
			}
		}
		transform_buffer_utilization: {
			tags: _component_tags & {
				output: _output
			}
		}
		transform_buffer_utilization_level: {
			tags: _component_tags & {
				output: _output
			}
		}
		transform_buffer_utilization_mean: {
			tags: _component_tags & {
				output: _output
			}
		}
		uptime_seconds: {
			tags:              _internal_metrics_tags
		}
		utf8_convert_errors_total: {
			tags: _component_tags & {
				mode: {
					description: "The connection mode used by the component."
					required:    true
					enum: {
						udp: "User Datagram Protocol"
					}
				}
			}
		}
		utilization: {
			tags:              _component_tags
		}
		build_info: {
			tags: _internal_metrics_tags & {
				debug: {
					description: "Whether this is a debug build of Vector"
					required:    true
				}
				version: {
					description: "Vector version."
					required:    true
				}
				rust_version: {
					description: "The Rust version from the package manifest."
					required:    true
				}
				arch: {
					description: "The target architecture being compiled for. (e.g. x86_64)"
					required:    true
				}
				revision: {
					description: "Revision identifer, related to versioned releases."
					required:    true
				}
			}
		}
		value_limit_reached_total: {
			tags:              _component_tags
		}

		// Windows metrics
		windows_service_install_total: {
			tags:              _internal_metrics_tags
		}
		windows_service_restart_total: {
			tags:              _internal_metrics_tags
		}
		windows_service_start_total: {
			tags:              _internal_metrics_tags
		}
		windows_service_stop_total: {
			tags:              _internal_metrics_tags
		}
		windows_service_uninstall_total: {
			tags:              _internal_metrics_tags
		}

		// config metrics
		config_reload_rejected: {
			tags: _internal_metrics_tags & {
				reason: _reason
			}
		}
		config_reloaded: {
			tags:              _internal_metrics_tags
		}

		// Helpful tag groupings
		_component_tags: _internal_metrics_tags & {
			component_kind: _component_kind
			component_id:   _component_id
			component_type: _component_type
		}

		// All available tags
		_collector: {
			required:    true
		}
		_component_kind: {
			required:    true
			enum: {
				"sink":      "Vector sink components"
				"source":    "Vector source components"
				"transform": "Vector transform components"
			}
		}
		_component_id: {
			required:    true
			examples: ["my_source", "my_sink"]
		}
		_component_type: {
			required:    true
			examples: ["file", "http", "honeycomb", "splunk_hec"]
		}
		_endpoint: {
			required:    true
			examples: ["http://localhost:8080/server-status?auto"]
		}
		_error_type: {
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
			required:    false
		}
		_grpc_method: {
			required:    true
		}
		_grpc_service: {
			required:    true
		}
		_grpc_status: {
			required:    true
		}
		_host: {
			required:    true
			examples: [_values.local_host]
		}
		_mode: {
			required:    false
			enum: {
				udp:  "User Datagram Protocol"
				tcp:  "Transmission Control Protocol"
				unix: "Unix domain socket"
			}
		}
		_output: {
			required:    false
		}
		_stage: {
			required:    true
			enum: {
				receiving:  "While receiving data."
				processing: "While processing data within the component."
				sending:    "While sending data."
			}
		}
		_status: {
			required:    false
		}
		_method: {
			required:    false
		}
		_path: {
			required:    true
		}
		_reason: {
			required:    true
			enum: {
				"out_of_order": "The event was out of order."
				"oversized":    "The event was too large."
			}
		}
	}

	how_it_works: {
		unique_series: {
			title: "Sending metrics from multiple Vector instances"
			body: """
				When sending `internal_metrics` from multiple Vector instances
				to the same destination, you will typically want to tag the
				metrics with a tag that is unique to the Vector instance sending
				the metrics to avoid the metric series conflicting. The
				`tags.host_key` option can be used for this, but you can also
				use a subsequent `remap` transform to add a different unique
				tag from the environment.
				"""
		}
	}
}
