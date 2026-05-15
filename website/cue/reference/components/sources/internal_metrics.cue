package metadata

components: sources: internal_metrics: {
	title: "Internal Metrics"


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
				required:    false
				examples: ["4232"]
			}
			host: {
				required:    false
				examples: [_values.local_host]
			}
		}

		// Instance-level "process" metrics
		active_clients: {
			type:              "gauge"
			default_namespace: "vector"
			tags:              _component_tags
		}
		aggregate_events_recorded_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		aggregate_failed_updates: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		aggregate_flushes_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		api_started_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		component_timed_out_events_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		component_timed_out_requests_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		connection_established_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		connection_send_errors_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		connection_shutdown_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		quit_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		reloaded_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		started_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		stopped_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}

		// Metrics emitted by one or more components
		// Reusable metric definitions
		adaptive_concurrency_averaged_rtt: {
			type:              "histogram"
			default_namespace: "vector"
			tags:              _component_tags
		}
		adaptive_concurrency_in_flight: {
			type:              "histogram"
			default_namespace: "vector"
			tags:              _component_tags
		}
		adaptive_concurrency_limit: {
			type:              "histogram"
			default_namespace: "vector"
			tags:              _component_tags
		}
		adaptive_concurrency_observed_rtt: {
			type:              "histogram"
			default_namespace: "vector"
			tags:              _component_tags
		}
		checkpoints_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		checksum_errors_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _internal_metrics_tags & {
				file: _file
			}
		}
		collect_completed_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		collect_duration_seconds: {
			type:              "histogram"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		command_executed_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		command_execution_duration_seconds: {
			type:              "histogram"
			default_namespace: "vector"
			tags:              _component_tags
		}
		connection_read_errors_total: {
			type:              "counter"
			default_namespace: "vector"
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
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		containers_unwatched_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		containers_watched_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		doris_bytes_loaded_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		doris_rows_filtered_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		doris_rows_loaded_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		k8s_format_picker_edge_cases_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		k8s_docker_format_parse_failures_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		events_discarded_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _internal_metrics_tags & {
				reason: _reason
			}
		}
		component_latency_seconds: {
			type:              "histogram"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		component_latency_mean_seconds: {
			type:              "gauge"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		buffer_byte_size: {
			type:               "gauge"
			default_namespace:  "vector"
			tags:               _component_tags
		}
		buffer_events: {
			type:               "gauge"
			default_namespace:  "vector"
			tags:               _component_tags
		}
		buffer_size_bytes: {
			type:              "gauge"
			default_namespace: "vector"
			tags:              _component_tags
		}
		buffer_size_events: {
			type:              "gauge"
			default_namespace: "vector"
			tags:              _component_tags
		}
		buffer_discarded_events_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		buffer_received_event_bytes_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		buffer_received_events_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		buffer_send_duration_seconds: {
			type:              "histogram"
			default_namespace: "vector"
			tags:              _component_tags
		}
		buffer_sent_event_bytes_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		buffer_sent_events_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		component_discarded_events_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _component_tags & {
				intentional: {
					required:    true
				}
			}
		}
		component_errors_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _component_tags & {
				error_type: _error_type
				stage:      _stage
			}
		}
		component_received_bytes_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              component_received_events_total.tags
		}
		component_received_bytes: {
			type:              "histogram"
			default_namespace: "vector"
			tags:              component_received_events_total.tags
		}
		component_received_events_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _component_tags & {
				file: {
					required:    false
				}
				uri: {
					required:    false
				}
				container_name: {
					required:    false
				}
				pod_name: {
					required:    false
				}
				peer_addr: {
					required:    false
				}
				peer_path: {
					required:    false
				}
				mode: _mode
			}
		}
		component_received_events_count: {
			type:              "histogram"
			default_namespace: "vector"
			tags: _component_tags & {
				file: {
					required:    false
				}
				uri: {
					required:    false
				}
				container_name: {
					required:    false
				}
				pod_name: {
					required:    false
				}
				peer_addr: {
					required:    false
				}
				peer_path: {
					required:    false
				}
				mode: _mode
			}
		}
		component_received_event_bytes_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              component_received_events_total.tags
		}
		component_sent_bytes_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _component_tags & {
				endpoint: {
					required:    false
				}
				file: {
					required:    false
				}
				protocol: {
					required:    true
				}
				region: {
					required:    false
				}
			}
		}
		component_sent_events_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _component_tags & {output: _output}
		}
		component_sent_event_bytes_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _component_tags & {output: _output}
		}
		internal_metrics_cardinality: {
			type:              "gauge"
			default_namespace: "vector"
			tags: {}
		}
		internal_metrics_cardinality_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              internal_metrics_cardinality.tags
		}
		kafka_queue_messages: {
			type:              "gauge"
			default_namespace: "vector"
			tags:              _component_tags
		}
		kafka_queue_messages_bytes: {
			type:              "gauge"
			default_namespace: "vector"
			tags:              _component_tags
		}
		kafka_requests_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		kafka_requests_bytes_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		kafka_responses_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		kafka_responses_bytes_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		kafka_produced_messages_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		kafka_produced_messages_bytes_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		kafka_consumed_messages_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		kafka_consumed_messages_bytes_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		kafka_consumer_lag: {
			type:              "gauge"
			default_namespace: "vector"
			tags: _component_tags & {
				topic_id: {
					required:    true
				}
				partition_id: {
					required:    true
				}
			}
		}
		files_added_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _internal_metrics_tags & {
				file: _file
			}
		}
		files_deleted_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _internal_metrics_tags & {
				file: _file
			}
		}
		files_resumed_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _internal_metrics_tags & {
				file: _file
			}
		}
		files_unwatched_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _internal_metrics_tags & {
				file: _file
			}
		}
		open_files: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		grpc_server_messages_received_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _component_tags & {
				grpc_method:  _grpc_method
				grpc_service: _grpc_service
			}
		}
		grpc_server_messages_sent_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _component_tags & {
				grpc_method:  _grpc_method
				grpc_service: _grpc_service
				grpc_status:  _grpc_status
			}
		}
		grpc_server_handler_duration_seconds: {
			type:              "histogram"
			default_namespace: "vector"
			tags: _component_tags & {
				grpc_method:  _grpc_method
				grpc_service: _grpc_service
				grpc_status:  _grpc_status
			}
		}
		http_client_response_rtt_seconds: {
			type:              "histogram"
			default_namespace: "vector"
			tags: _component_tags & {
				status: _status
			}
		}
		http_client_requests_sent_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _component_tags & {
				method: _method
			}
		}
		http_client_responses_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _component_tags & {
				status: _status
			}
		}
		http_client_rtt_seconds: {
			type:              "histogram"
			default_namespace: "vector"
			tags:              _component_tags
		}
		http_requests_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		http_server_requests_received_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _component_tags & {
				method: _method
				path:   _path
			}
		}
		http_server_responses_sent_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _component_tags & {
				method: _method
				path:   _path
				status: _status
			}
		}
		http_server_handler_duration_seconds: {
			type:              "histogram"
			default_namespace: "vector"
			tags: _component_tags & {
				method: _method
				path:   _path
				status: _status
			}
		}
		invalid_record_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		lua_memory_used_bytes: {
			type:              "gauge"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		metadata_refresh_failed_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		metadata_refresh_successful_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		open_connections: {
			type:              "gauge"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		protobuf_decode_errors_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		send_errors_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		source_lag_time_seconds: {
			type:              "histogram"
			default_namespace: "vector"
			tags:              _component_tags
		}
		source_send_batch_latency_seconds: {
			type:              "histogram"
			default_namespace: "vector"
			tags:              _component_tags
		}
		source_send_latency_seconds: {
			type:              "histogram"
			default_namespace: "vector"
			tags:              _component_tags
		}
		source_buffer_max_byte_size: {
			type:              "gauge"
			default_namespace: "vector"
			tags: _component_tags & {
				output: _output
			}
		}
		source_buffer_max_event_size: {
			type:              "gauge"
			default_namespace: "vector"
			tags: _component_tags & {
				output: _output
			}
		}
		source_buffer_max_size_bytes: {
			type:              "gauge"
			default_namespace: "vector"
			tags: _component_tags & {
				output: _output
			}
		}
		source_buffer_max_size_events: {
			type:              "gauge"
			default_namespace: "vector"
			tags: _component_tags & {
				output: _output
			}
		}
		source_buffer_utilization: {
			type:              "histogram"
			default_namespace: "vector"
			tags: _component_tags & {
				output: _output
			}
		}
		source_buffer_utilization_level: {
			type:              "gauge"
			default_namespace: "vector"
			tags: _component_tags & {
				output: _output
			}
		}
		source_buffer_utilization_mean: {
			type:              "gauge"
			default_namespace: "vector"
			tags: _component_tags & {
				output: _output
			}
		}
		splunk_pending_acks: {
			type:              "gauge"
			default_namespace: "vector"
			tags:              _component_tags
		}
		streams_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		s3_object_processing_failed_duration_seconds: {
			type:              "histogram"
			default_namespace: "vector"
			tags: _component_tags & {
				bucket: {
					required:    true
				}
			}
		}
		s3_object_processing_succeeded_duration_seconds: {
			type:              "histogram"
			default_namespace: "vector"
			tags: _component_tags & {
				bucket: {
					required:    true
				}
			}
		}
		sqs_message_delete_succeeded_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		sqs_message_processing_succeeded_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		sqs_message_receive_succeeded_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		sqs_message_received_messages_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		sqs_s3_event_record_ignored_total: {
			type:              "counter"
			default_namespace: "vector"

			tags: _component_tags & {
				ignore_type: {
					required:    true
					enum: {
						"invalid_event_kind": "The kind of invalid event."
					}
				}
			}
		}
		stale_events_flushed_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		stdin_reads_failed_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		tag_value_limit_exceeded_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _component_tags & {
				metric_name: {
					required: false
				}
				tag_key: {
					required: false
				}
			}
		}
		timestamp_parse_errors_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}
		transform_buffer_max_byte_size: {
			type:              "gauge"
			default_namespace: "vector"
			tags: _component_tags & {
				output: _output
			}
		}
		transform_buffer_max_event_size: {
			type:              "gauge"
			default_namespace: "vector"
			tags: _component_tags & {
				output: _output
			}
		}
		transform_buffer_max_size_bytes: {
			type:              "gauge"
			default_namespace: "vector"
			tags: _component_tags & {
				output: _output
			}
		}
		transform_buffer_max_size_events: {
			type:              "gauge"
			default_namespace: "vector"
			tags: _component_tags & {
				output: _output
			}
		}
		transform_buffer_utilization: {
			type:              "histogram"
			default_namespace: "vector"
			tags: _component_tags & {
				output: _output
			}
		}
		transform_buffer_utilization_level: {
			type:              "gauge"
			default_namespace: "vector"
			tags: _component_tags & {
				output: _output
			}
		}
		transform_buffer_utilization_mean: {
			type:              "gauge"
			default_namespace: "vector"
			tags: _component_tags & {
				output: _output
			}
		}
		uptime_seconds: {
			type:              "gauge"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		utf8_convert_errors_total: {
			type:              "counter"
			default_namespace: "vector"
			tags: _component_tags & {
				mode: {
					required:    true
					enum: {
						udp: "User Datagram Protocol"
					}
				}
			}
		}
		utilization: {
			type:              "gauge"
			default_namespace: "vector"
			tags:              _component_tags
		}
		build_info: {
			type:              "gauge"
			default_namespace: "vector"
			tags: _internal_metrics_tags & {
				debug: {
					required:    true
				}
				version: {
					required:    true
				}
				rust_version: {
					required:    true
				}
				arch: {
					required:    true
				}
				revision: {
					required:    true
				}
			}
		}
		value_limit_reached_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _component_tags
		}

		// Windows metrics
		windows_service_install_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		windows_service_restart_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		windows_service_start_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		windows_service_stop_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}
		windows_service_uninstall_total: {
			type:              "counter"
			default_namespace: "vector"
			tags:              _internal_metrics_tags
		}

		// config metrics
		config_reload_rejected: {
			type:              "counter"
			default_namespace: "vector"
			tags: _internal_metrics_tags & {
				reason: _reason
			}
		}
		config_reloaded: {
			type:              "counter"
			default_namespace: "vector"
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
