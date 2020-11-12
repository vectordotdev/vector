package metadata

// Container metrics
_vector_communication_errors_total: {
	description: "The total number of errors stemming from communication with the Docker daemon."
	type:        "counter"
	tags:        _component_tags
}

_vector_container_events_processed_total: {
	description: "The total number of container events processed."
	type:        "counter"
	tags:        _component_tags
}

_vector_container_metadata_fetch_errors_total: {
	description: "The total number of errors caused by failure to fetch container metadata."
	type:        "counter"
	tags:        _component_tags
}

_vector_containers_unwatched_total: {
	description: "The total number of times Vector stopped watching for container logs."
	type:        "counter"
	tags:        _component_tags
}

_vector_containers_watched_total: {
	description: "The total number of times Vector started watching for container logs."
	type:        "counter"
	tags:        _component_tags
}

_vector_logging_driver_errors_total: {
	description: "The total number of logging driver errors encountered caused by not using either the `jsonfile` or `journald` driver."
	type:        "counter"
	tags:        _component_tags
}

// Kubernetes metrics
_vector_k8s_docker_format_parse_failures_total: {
	description: "The total number of failures to parse a message as a JSON object."
	type:        "counter"
	tags:        _component_tags
}

_vector_k8s_event_annotation_failures_total: {
	description: "The total number of failures to annotate Vector events with Kubernetes Pod metadata."
	type:        "counter"
	tags:        _component_tags
}

// Vector internal metrics (plus misc)
_vector_api_started_total: {
	description: "The number of times the Vector GraphQL API has been started."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_auto_concurrency_averaged_rtt: {
	description: "The average round-trip time (RTT) from the HTTP sink across the current window."
	type:        "histogram"
	tags:        _internal_metrics_tags
}
_vector_auto_concurrency_in_flight: {
	description: "The number of outbound requests from the HTTP sink currently awaiting a response."
	type:        "histogram"
	tags:        _internal_metrics_tags
}
_vector_auto_concurrency_limit: {
	description: "The concurrency limit that the auto-concurrency feature has decided on for this current window."
	type:        "histogram"
	tags:        _internal_metrics_tags
}
_vector_auto_concurrency_observed_rtt: {
	description: "The observed round-trip time (RTT) for requests from this HTTP sink."
	type:        "histogram"
	tags:        _internal_metrics_tags
}
_vector_checkpoint_write_errors_total: {
	description: "The total number of errors writing checkpoints."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_checkpoints_total: {
	description: "The total number of files checkpointed."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_checksum_errors: {
	description: "The total number of errors identifying files via checksum."
	type:        "counter"
	tags:        _internal_metrics_tags & {
		file: _file
	}
}
_vector_events_discarded_total: {
	description: "The total number of events discarded by this component."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_events_processed_total: {
	description: "The total number of events processed by this component."
	type:        "counter"
	tags:        _component_tags & {
		file: _file
	}
}
_vector_file_delete_errors: {
	description: "The total number of failures to delete a file."
	type:        "counter"
	tags:        _internal_metrics_tags & {
		file: _file
	}
}
_vector_file_watch_errors: {
	description: "The total number of errors caused by failure to watch a file."
	type:        "counter"
	tags:        _internal_metrics_tags & {
		file: _file
	}
}
_vector_files_added: {
	description: "The total number of files Vector has found to watch."
	type:        "counter"
	tags:        _internal_metrics_tags & {
		file: _file
	}
}
_vector_files_deleted: {
	description: "The total number of files deleted."
	type:        "counter"
	tags:        _internal_metrics_tags & {
		file: _file
	}
}
_vector_files_resumed: {
	description: "The total number of times Vector has resumed watching a file."
	type:        "counter"
	tags:        _internal_metrics_tags & {
		file: _file
	}
}
_vector_files_unwatched: {
	description: "The total number of times Vector has stopped watching a file."
	type:        "counter"
	tags:        _internal_metrics_tags & {
		file: _file
	}
}
_vector_fingerprint_read_errors: {
	description: "The total number of times failing to read a file for fingerprinting."
	type:        "counter"
	tags:        _internal_metrics_tags & {
		file: _file
	}
}
_vector_http_bad_requests_total: {
	description: "The total number of HTTP `400 Bad Request` errors encountered."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_http_error_response_total: {
	description: "The total number of HTTP error responses for this component."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_http_request_errors_total: {
	description: "The total number of HTTP request errors for this component."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_http_requests_total: {
	description: "The total number of HTTP requests issued by this component."
	type:        "counter"
	tags:        _component_tags
}
_vector_memory_used: {
	description: "The total memory currently being used by Vector (in bytes)."
	type:        "gauge"
	tags:        _internal_metrics_tags
}
_vector_missing_keys_total: {
	description: "The total number of events dropped due to keys missing from the event."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_open_connections: {
	description: "The number of current open connections to Vector."
	type:        "gauge"
	tags:        _internal_metrics_tags
}
_vector_parse_errors_total: {
	description: "The total number of errors parsing Prometheus metrics."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_processed_bytes_total: {
	description: "The total number of bytes processed by the component."
	type:        "counter"
	tags:        _component_tags
}
_vector_processing_errors_total: {
	description: "The total number of processing errors encountered by this component."
	type:        "counter"
	tags:        _component_tags & {
		error_type: _error_type
	}
}
_vector_protobuf_decode_errors_total: {
	description: "The total number of [Protocol Buffers](\(urls.protobuf)) errors thrown during communication between Vector instances."
	type:        "counter"
	tags:        _component_tags
}
_vector_request_duration_nanoseconds: {
	description: "The request duration for this component (in nanoseconds)."
	type:        "histogram"
	tags:        _component_tags
}
_vector_request_read_errors_total: {
	description: "The total number of request read errors for this component."
	type:        "counter"
	tags:        _component_tags
}
_vector_requests_completed_total: {
	description: "The total number of requests completed by this component."
	type:        "counter"
	tags:        _component_tags
}
_vector_requests_received_total: {
	description: "The total number of requests received by this component."
	type:        "counter"
	tags:        _component_tags
}
_vector_timestamp_parse_errors_total: {
	description: "The total number of errors encountered parsing [RFC3339](\(urls.rfc_3339)) timestamps."
	type:        "counter"
	tags:        _component_tags
}
_vector_uptime_seconds: {
	description: "The total number of seconds the Vector instance has been up."
	type:        "gauge"
	tags:        _component_tags
}

// Splunk
_vector_encode_errors_total: {
	description: "The total number of errors encoding [Splunk HEC](\(urls.splunk_hec_protocol)) events to JSON for this `splunk_hec` sink."
	type:        "counter"
	tags:        _component_tags
}
_vector_source_missing_keys_total: {
	description: "The total number of errors rendering the template for this source."
	type:        "counter"
	tags:        _component_tags
}
_vector_sourcetype_missing_keys_total: {
	description: "The total number of errors rendering the template for this sourcetype."
	type:        "counter"
	tags:        _component_tags
}

// Vector instance metrics
_vector_config_load_errors_total: {
	description: "The total number of errors loading the Vector configuration."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_connection_errors_total: {
	description: "The total number of connection errors for this Vector instance."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_quit_total: {
	description: "The total number of times the Vector instance has quit."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_recover_errors_total: {
	description: "The total number of errors caused by Vector failing to recover from a failed reload."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_reload_errors_total: {
	description: "The total number of errors encountered when reloading Vector."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_reloaded_total: {
	description: "The total number of times the Vector instance has been reloaded."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_started_total: {
	description: "The total number of times the Vector instance has been started."
	type:        "counter"
	tags:        _internal_metrics_tags
}
_vector_stopped_total: {
	description: "The total number of times the Vector instance has been stopped."
	type:        "counter"
	tags:        _internal_metrics_tags
}

// Windows metrics
_windows_service_does_not_exist: {
	description: """
		The total number of errors raised due to the Windows service not
		existing.
		"""
	type: "counter"
	tags: _internal_metrics_tags
}
_windows_service_install: {
	description: """
		The total number of times the Windows service has been installed.
		"""
	type: "counter"
	tags: _internal_metrics_tags
}
_windows_service_restart: {
	description: """
		The total number of times the Windows service has been restarted.
		"""
	type: "counter"
	tags: _internal_metrics_tags
}
_windows_service_start: {
	description: """
		The total number of times the Windows service has been started.
		"""
	type: "counter"
	tags: _internal_metrics_tags
}
_windows_service_stop: {
	description: """
		The total number of times the Windows service has been stopped.
		"""
	type: "counter"
	tags: _internal_metrics_tags
}
_windows_service_uninstall: {
	description: """
		The total number of times the Windows service has been uninstalled.
		"""
	type: "counter"
	tags: _internal_metrics_tags
}

// All available tags
_collector: {
	description: "Which collector this metric comes from."
	required:    true
}
_component_kind: {
	description: "The component's kind (options are `source`, `sink`, or `transform`)."
	required:    true
	options: ["sink", "source", "transform"]
}
_component_name: {
	description: "The name of the component as specified in the Vector configuration."
	required:    true
	examples: ["file_source", "splunk_sink"]
}
_component_type: {
	description: "The type of component (source, transform, or sink)."
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
	options: [
		"field_missing",
		"invalid_metric",
		"mapping_failed",
		"match_failed",
		"parse_failed",
		"render_error",
		"type_conversion_failed",
		"value_invalid",
	]
}
_file: {
	description: "The file that produced the error"
	required:    false
}
_host: {
	description: "The hostname of the originating system."
	required:    true
	examples: [_values.local_host]
}
_instance: {
	description: "The Vector instance identified by host and port."
	required:    true
	examples: [_values.instance]
}
_job: {
	description: "The name of the job producing Vector metrics."
	required:    true
	default:     "vector"
}

// Convenient groupings of tags
_component_tags: _internal_metrics_tags & {
	component_kind: _component_kind
	component_name: _component_name
	component_type: _component_type
	instance:       _instance
	job:            _job
}

_apache_metrics_tags: {
	endpoint: _endpoint
	host: {
		description: "The hostname of the Apache HTTP server."
		required:    true
		examples: [_values.local_host]
	}
}
_host_metrics_tags: {
	collector: _collector
	host:      _host
}
_internal_metrics_tags: {
	instance: _instance
	job:      _job
}

// Helpful metrics groupings
_internal_metrics: {
	vector_config_load_errors_total: _vector_config_load_errors_total
	vector_quit_total:               _vector_quit_total
	vector_recover_errors_total:     _vector_recover_errors_total
	vector_reload_errors_total:      _vector_reload_errors_total
	vector_reloaded_total:           _vector_reloaded_total
	vector_started_total:            _vector_started_total
	vector_stopped_total:            _vector_stopped_total
}

_prometheus_metrics: {
	vector_events_processed_total:       _vector_events_processed_total
	vector_http_error_response_total:    _vector_http_error_response_total
	vector_http_request_errors_total:    _vector_http_request_errors_total
	vector_parse_errors_total:           _vector_parse_errors_total
	vector_processed_bytes_total:        _vector_processed_bytes_total
	vector_request_duration_nanoseconds: _vector_request_duration_nanoseconds
	vector_requests_completed_total:     _vector_requests_completed_total
}
