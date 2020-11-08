package metadata

// Apache metrics
_apache_access_total: {
	description:   "The total number of time the Apache server has been accessed."
	relevant_when: "`ExtendedStatus On`"
	type:          "counter"
	tags:          _apache_metrics_tags
}
_apache_connections: {
	description: "The total number of time the Apache server has been accessed."
	type:        "gauge"
	tags:        _apache_metrics_tags & {
		state: {
			description: "The state of the connection"
			required:    true
			examples: ["closing", "keepalive", "total", "writing"]
		}
	}
}
_apache_cpu_load: {
	description:   "The current CPU of the Apache server."
	relevant_when: "`ExtendedStatus On`"
	type:          "gauge"
	tags:          _apache_metrics_tags
}
_apache_cpu_seconds_total: {
	description:   "The CPU time of various Apache processes."
	relevant_when: "`ExtendedStatus On`"
	type:          "counter"
	tags:          _apache_metrics_tags & {
		state: {
			description: "The state of the connection"
			required:    true
			examples: ["children_system", "children_user", "system", "user"]
		}
	}
}
_apache_duration_seconds_total: {
	description:   "The amount of time the Apache server has been running."
	relevant_when: "`ExtendedStatus On`"
	type:          "counter"
	tags:          _apache_metrics_tags
}
_apache_scoreboard: {
	description: "The amount of times various Apache server tasks have been run."
	type:        "gauge"
	tags:        _apache_metrics_tags & {
		state: {
			description: "The connect state"
			required:    true
			examples: ["closing", "dnslookup", "finishing", "idle_cleanup", "keepalive", "logging", "open", "reading", "sending", "starting", "waiting"]
		}
	}
}
_apache_sent_bytes_total: {
	description:   "The amount of bytes sent by the Apache server."
	relevant_when: "`ExtendedStatus On`"
	type:          "counter"
	tags:          _apache_metrics_tags
}
_apache_up: {
	description: "If the Apache server is up or not."
	type:        "gauge"
	tags:        _apache_metrics_tags
}
_apache_uptime_seconds_total: {
	description: "The amount of time the Apache server has been running."
	type:        "counter"
	tags:        _apache_metrics_tags
}
_apache_workers: {
	description: "Apache worker statuses."
	type:        "gauge"
	tags:        _apache_metrics_tags & {
		state: {
			description: "The state of the worker"
			required:    true
			examples: ["busy", "idle"]
		}
	}
}

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

// Host metrics
// Host CPU
_host_cpu_seconds_total: {
	description: "The number of CPU seconds accumulated in different operating modes."
	type:        "counter"
	tags:        _host_metrics_tags & {
		collector: examples: ["cpu"]
		cpu: {
			description: "The index of the CPU core or socket."
			required:    true
			examples: ["1"]
		}
		mode: {
			description: "Which mode the CPU was running in during the given time."
			required:    true
			examples: ["idle", "system", "user", "nice"]
		}
	}
}

// Host disk
_host_disk_read_bytes_total:       _disk_counter & {description: "The accumulated number of bytes read in."}
_host_disk_reads_completed_total:  _disk_counter & {description: "The accumulated number of read operations completed."}
_host_disk_written_bytes_total:    _disk_counter & {description: "The accumulated number of bytes written out."}
_host_disk_writes_completed_total: _disk_counter & {description: "The accumulated number of write operations completed."}

// Host filesystem
_host_filesystem_free_bytes:  _filesystem_bytes & {description: "The number of bytes free on the named filesystem."}
_host_filesystem_total_bytes: _filesystem_bytes & {description: "The total number of bytes in the named filesystem."}
_host_filesystem_used_bytes:  _filesystem_bytes & {description: "The number of bytes used on the named filesystem."}

// Host load
_host_load1:  _loadavg & {description: "System load averaged over the last 1 second."}
_host_load5:  _loadavg & {description: "System load averaged over the last 5 seconds."}
_host_load15: _loadavg & {description: "System load averaged over the last 15 seconds."}

// Host memory
_host_memory_active_bytes:           _memory_gauge & _memory_nowin & {description: "The number of bytes of active main memory."}
_host_memory_available_bytes:        _memory_gauge & {description:                 "The number of bytes of main memory available."}
_host_memory_buffers_bytes:          _memory_linux & {description:                 "The number of bytes of main memory used by buffers."}
_host_memory_cached_bytes:           _memory_linux & {description:                 "The number of bytes of main memory used by cached blocks."}
_host_memory_free_bytes:             _memory_gauge & {description:                 "The number of bytes of main memory not used."}
_host_memory_inactive_bytes:         _memory_macos & {description:                 "The number of bytes of main memory that is not active."}
_host_memory_shared_bytes:           _memory_linux & {description:                 "The number of bytes of main memory shared between processes."}
_host_memory_swap_free_bytes:        _memory_gauge & {description:                 "The number of free bytes of swap space."}
_host_memory_swapped_in_bytes_total: _memory_counter & _memory_nowin & {
	description: "The number of bytes that have been swapped in to main memory."
}
_host_memory_swapped_out_bytes_total: _memory_counter & _memory_nowin & {
	description: "The number of bytes that have been swapped out from main memory."
}
_host_memory_swap_total_bytes: _memory_gauge & {description: "The total number of bytes of swap space."}
_host_memory_swap_used_bytes:  _memory_gauge & {description: "The number of used bytes of swap space."}
_host_memory_total_bytes:      _memory_gauge & {description: "The total number of bytes of main memory."}
_host_memory_used_bytes:       _memory_linux & {description: "The number of bytes of main memory used by programs or caches."}
_host_memory_wired_bytes:      _memory_macos & {description: "The number of wired bytes of main memory."}

// Host network
_host_network_receive_bytes_total:         _network_gauge & {description: "The number of bytes received on this interface."}
_host_network_receive_errs_total:          _network_gauge & {description: "The number of errors encountered during receives on this interface."}
_host_network_receive_packets_total:       _network_gauge & {description: "The number of packets received on this interface."}
_host_network_transmit_bytes_total:        _network_gauge & {description: "The number of bytes transmitted on this interface."}
_host_network_transmit_errs_total:         _network_gauge & {description: "The number of errors encountered during transmits on this interface."}
_host_network_transmit_packets_drop_total: _network_nomac & {description: "The number of packets dropped during transmits on this interface."}
_host_network_transmit_packets_total:      _network_nomac & {description: "The number of packets transmitted on this interface."}

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

// MongoDB metrics
_mongodb_assets_total: {
	description: "Number of assertions raised since the MongoDB process started."
	type:        "counter"
	tags:        _mongodb_metrics_tags & {
		type: {
			description: "The assertion type"
			required:    true
			examples: ["regular", "warning", "msg", "user", "rollovers"]
		}
	}
}
_mongodb_bson_parse_error_total: {
	description: "The total number of BSON parsing errors."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_connections: {
	description: "Number of connections in some state."
	type:        "gauge"
	tags:        _mongodb_metrics_tags & {
		state: {
			description: "The connection state"
			required:    true
			examples: ["active", "available", "current"]
		}
	}
}
_mongodb_extra_info_heap_usage_bytes: {
	description:   "The total size in bytes of heap space used by the database process."
	relevant_when: "Unix/Linux"
	type:          "gauge"
	tags:          _mongodb_metrics_tags
}
_mongodb_extra_info_page_faults: {
	description: "The total number of page faults."
	type:        "gauge"
	tags:        _mongodb_metrics_tags
}
_mongodb_instance_local_time: {
	description: "The ISODate representing the current time, according to the server, in UTC."
	type:        "gauge"
	tags:        _mongodb_metrics_tags
}
_mongodb_instance_uptime_estimate_seconds_total: {
	description: "The uptime in seconds as calculated from MongoDB’s internal course-grained time keeping system."
	type:        "gauge"
	tags:        _mongodb_metrics_tags
}
_mongodb_instance_uptime_seconds_total: {
	description: "The number of seconds that the current MongoDB process has been active."
	type:        "gauge"
	tags:        _mongodb_metrics_tags
}
_mongodb_memory: {
	description: "Current memory unsage."
	type:        "gauge"
	tags:        _mongodb_metrics_tags & {
		type: {
			description: "Memory type"
			required:    true
			examples: ["resident", "virtual", "mapped", "mapped_with_journal"]
		}
	}
}
_mongodb_mongod_global_lock_active_clients: {
	description: "Number of connected clients and the read and write operations performed by these clients."
	type:        "gauge"
	tags:        _mongodb_metrics_tags & {
		type: {
			description: "Number type."
			required:    true
			examples: ["total", "readers", "writers"]
		}
	}
}
_mongodb_mongod_global_lock_current_queue: {
	description: "Number of operations queued because of a lock."
	type:        "gauge"
	tags:        _mongodb_metrics_tags & {
		type: {
			description: "Number type."
			required:    true
			examples: ["total", "readers", "writers"]
		}
	}
}
_mongodb_mongod_global_lock_total_time_seconds: {
	description: "The time since the database last started and created the globalLock. This is roughly equivalent to total server uptime."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_locks_time_acquiring_global_seconds_total: {
	description: "Amount of time that any database has spent waiting for the global lock."
	type:        "counter"
	tags:        _mongodb_metrics_tags & {
		type: {
			description: "Lock type."
			required:    true
			examples: ["ParallelBatchWriterMode", "ReplicationStateTransition", "Global", "Database", "Collection", "Mutex", "Metadata", "oplog"]
		}
		mode: {
			description: "Lock mode."
			required:    true
			examples: ["read", "write"]
		}
	}
}
_mongodb_mongod_metrics_cursor_open: {
	description: "Number of cursors."
	type:        "gauge"
	tags:        _mongodb_metrics_tags & {
		state: {
			description: "Cursor state."
			required:    true
			examples: ["no_timeout", "pinned", "total"]
		}
	}
}
_mongodb_mongod_metrics_cursor_timed_out_total: {
	description: "The total number of cursors that have timed out since the server process started."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_document_total: {
	description: "Document access and modification patterns."
	type:        "counter"
	tags:        _mongodb_metrics_tags & {
		state: {
			description: "Document state."
			required:    true
			examples: ["deleted", "inserted", "returned", "updated"]
		}
	}
}
_mongodb_mongod_metrics_get_last_error_wtime_num: {
	description: "The total number of getLastError operations with a specified write concern."
	type:        "gauge"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_get_last_error_wtime_seconds_total: {
	description: "The total amount of time that the mongod has spent performing getLastError operations."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_get_last_error_wtimeouts_total: {
	description: "The number of times that write concern operations have timed out as a result of the wtimeout threshold to getLastError."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_operation_total: {
	description: "Update and query operations that MongoDB handles using special operation types."
	type:        "counter"
	tags:        _mongodb_metrics_tags & {
		type: {
			description: "Operation type."
			required:    true
			examples: ["scan_and_order", "write_conflicts"]
		}
	}
}
_mongodb_mongod_metrics_query_executor_total: {
	description: "Data from query execution system."
	type:        "counter"
	tags:        _mongodb_metrics_tags & {
		state: {
			description: "Query state."
			required:    true
			examples: ["scanned", "scanned_objects", "collection_scans"]
		}
	}
}
_mongodb_mongod_metrics_record_moves_total: {
	description: "Moves reports the total number of times documents move within the on-disk representation of the MongoDB data set. Documents move as a result of operations that increase the size of the document beyond their allocated record size."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_repl_apply_batches_num_total: {
	description: "The total number of batches applied across all databases."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_repl_apply_batches_seconds_total: {
	description: "The total amount of time the mongod has spent applying operations from the oplog."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_repl_apply_ops_total: {
	description: "The total number of oplog operations applied."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_repl_buffer_count: {
	description: "The current number of operations in the oplog buffer."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_repl_buffer_max_size_bytes_total: {
	description: "The maximum size of the buffer."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_repl_buffer_size_bytes: {
	description: "The current size of the contents of the oplog buffer."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_repl_executor_queue: {
	description: "Number of queued operations in the replication executor."
	type:        "gauge"
	tags:        _mongodb_metrics_tags & {
		type: {
			description: "Queue type."
			required:    true
			examples: ["network_in_progress", "sleepers"]
		}
	}
}
_mongodb_mongod_metrics_repl_executor_unsignaled_events: {
	description: "Number of unsignaled events in the replication executor."
	type:        "gauge"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_repl_network_bytes_total: {
	description: "The total amount of data read from the replication sync source."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_repl_network_getmores_num_total: {
	description: "The total number of getmore operations, which are operations that request an additional set of operations from the replication sync source."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_repl_network_getmores_seconds_total: {
	description: "The total amount of time required to collect data from getmore operations."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_repl_network_ops_total: {
	description: "The total number of operations read from the replication source."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_repl_network_readers_created_total: {
	description: "The total number of oplog query processes created."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_ttl_deleted_documents_total: {
	description: "The total number of documents deleted from collections with a ttl index."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_metrics_ttl_passes_total: {
	description: "The number of times the background process removes documents from collections with a ttl index."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_op_latencies_histogram: {
	description: "Latency statistics."
	type:        "gauge"
	tags:        _mongodb_metrics_tags & {
		type: {
			description: "Latency type."
			required:    true
			examples: ["reads", "writes", "commands"]
		}
		micros: {
			description: "Bucket."
			required:    true
			examples: ["1", "2", "4096", "16384", "49152"]
		}
	}
}
_mongodb_mongod_op_latencies_latency: {
	description: "A 64-bit integer giving the total combined latency in microseconds."
	type:        "gauge"
	tags:        _mongodb_metrics_tags & {
		type: {
			description: "Latency type."
			required:    true
			examples: ["network_in_progress", "sleepers"]
		}
	}
}
_mongodb_mongod_op_latencies_ops_total: {
	description: "A 64-bit integer giving the total number of operations performed on the collection since startup."
	type:        "gauge"
	tags:        _mongodb_metrics_tags & {
		type: {
			description: "Latency type."
			required:    true
			examples: ["network_in_progress", "sleepers"]
		}
	}
}
_mongodb_mongod_storage_engine: {
	description: "The name of the current storage engine."
	type:        "gauge"
	tags:        _mongodb_metrics_tags & {
		engine: {
			description: "Engine name."
			required:    true
			examples: ["wiredTiger"]
		}
	}
}
_mongodb_mongod_wiredtiger_blockmanager_blocks_total: {
	description:   "Statistics on the block manager operations."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "counter"
	tags:          _mongodb_metrics_tags & {
		type: {
			description: "Operation type."
			required:    true
			examples: ["blocks_read", "blocks_read_mapped", "blocks_pre_loaded", "blocks_written"]
		}
	}
}
_mongodb_mongod_wiredtiger_blockmanager_bytes_total: {
	description:   "Statistics on the block manager operations."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "counter"
	tags:          _mongodb_metrics_tags & {
		type: {
			description: "Operation type."
			required:    true
			examples: ["bytes_read", "bytes_read_mapped", "bytes_written"]
		}
	}
}
_mongodb_mongod_wiredtiger_cache_bytes: {
	description:   "Statistics on the cache and page evictions from the cache."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "gauge"
	tags:          _mongodb_metrics_tags & {
		type: {
			description: "Statistics type."
			required:    true
			examples: ["total", "dirty", "internal_pages", "leaf_pages"]
		}
	}
}
_mongodb_mongod_wiredtiger_cache_bytes_total: {
	description:   "Statistics on the cache and page evictions from the cache."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "counter"
	tags:          _mongodb_metrics_tags & {
		type: {
			description: "Statistics type."
			required:    true
			examples: ["read", "written"]
		}
	}
}
_mongodb_mongod_wiredtiger_cache_evicted_total: {
	description:   "Statistics on the cache and page evictions from the cache."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "counter"
	tags:          _mongodb_metrics_tags & {
		type: {
			description: "Statistics type."
			required:    true
			examples: ["modified", "unmodified"]
		}
	}
}
_mongodb_mongod_wiredtiger_cache_max_bytes: {
	description: "Maximum cache size."
	type:        "gauge"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_wiredtiger_cache_overhead_percent: {
	description: "Percentage overhead."
	type:        "gauge"
	tags:        _mongodb_metrics_tags
}
_mongodb_mongod_wiredtiger_cache_pages: {
	description:   "Pages in the cache."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "gauge"
	tags:          _mongodb_metrics_tags & {
		type: {
			description: "Pages type."
			required:    true
			examples: ["total", "dirty"]
		}
	}
}
_mongodb_mongod_wiredtiger_cache_pages_total: {
	description:   "Pages in the cache."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "counter"
	tags:          _mongodb_metrics_tags & {
		type: {
			description: "Pages type."
			required:    true
			examples: ["read", "write"]
		}
	}
}
_mongodb_mongod_wiredtiger_concurrent_transactions_available_tickets: {
	description:   "Information on the number of concurrent of read and write transactions allowed into the WiredTiger storage engine"
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "gauge"
	tags:          _mongodb_metrics_tags & {
		type: {
			description: "Transactions type."
			required:    true
			examples: ["read", "write"]
		}
	}
}
_mongodb_mongod_wiredtiger_concurrent_transactions_out_tickets: {
	description:   "Information on the number of concurrent of read and write transactions allowed into the WiredTiger storage engine"
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "gauge"
	tags:          _mongodb_metrics_tags & {
		type: {
			description: "Transactions type."
			required:    true
			examples: ["read", "write"]
		}
	}
}
_mongodb_mongod_wiredtiger_concurrent_transactions_total_tickets: {
	description:   "Information on the number of concurrent of read and write transactions allowed into the WiredTiger storage engine"
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "gauge"
	tags:          _mongodb_metrics_tags & {
		type: {
			description: "Transactions type."
			required:    true
			examples: ["read", "write"]
		}
	}
}
_mongodb_mongod_wiredtiger_log_bytes_total: {
	description:   "Statistics on WiredTiger’s write ahead log (i.e. the journal)."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "counter"
	tags:          _mongodb_metrics_tags & {
		type: {
			description: "Bytes type."
			required:    true
			examples: ["payload", "written"]
		}
	}
}
_mongodb_mongod_wiredtiger_log_operations_total: {
	description:   "Statistics on WiredTiger’s write ahead log (i.e. the journal)."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "counter"
	tags:          _mongodb_metrics_tags & {
		type: {
			description: "Operations type."
			required:    true
			examples: ["write", "scan", "scan_double", "sync", "sync_dir", "flush"]
		}
	}
}
_mongodb_mongod_wiredtiger_log_records_scanned_total: {
	description:   "Statistics on WiredTiger’s write ahead log (i.e. the journal)."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "counter"
	tags:          _mongodb_metrics_tags & {
		type: {
			description: "Scanned records type."
			required:    true
			examples: ["compressed", "uncompressed"]
		}
	}
}
_mongodb_mongod_wiredtiger_log_records_total: {
	description:   "Statistics on WiredTiger’s write ahead log (i.e. the journal)."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "counter"
	tags:          _mongodb_metrics_tags
}
_mongodb_mongod_wiredtiger_session_open_sessions: {
	description:   "Open session count."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "counter"
	tags:          _mongodb_metrics_tags
}
_mongodb_mongod_wiredtiger_transactions_checkpoint_seconds: {
	description:   "Statistics on transaction checkpoints and operations."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "gauge"
	tags:          _mongodb_metrics_tags & {
		type: {
			description: "Checkpoint type."
			required:    true
			examples: ["min", "max"]
		}
	}
}
_mongodb_mongod_wiredtiger_transactions_checkpoint_seconds_total: {
	description:   "Statistics on transaction checkpoints and operations."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "counter"
	tags:          _mongodb_metrics_tags
}
_mongodb_mongod_wiredtiger_transactions_running_checkpoints: {
	description:   "Statistics on transaction checkpoints and operations."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "counter"
	tags:          _mongodb_metrics_tags
}
_mongodb_mongod_wiredtiger_transactions_total: {
	description:   "Statistics on transaction checkpoints and operations."
	relevant_when: "Storage engine is `wiredTiger`."
	type:          "counter"
	tags:          _mongodb_metrics_tags & {
		type: {
			description: "Transactions type."
			required:    true
			examples: ["begins", "checkpoints", "committed", "rolledback"]
		}
	}
}
_mongodb_network_bytes_total: {
	description: "The number of bytes that reflects the amount of network traffic."
	type:        "counter"
	tags:        _mongodb_metrics_tags & {
		state: {
			description: "Bytes state."
			required:    true
			examples: ["bytes_in", "bytes_out"]
		}
	}
}
_mongodb_network_metrics_num_requests_total: {
	description: "The total number of distinct requests that the server has received."
	type:        "counter"
	tags:        _mongodb_metrics_tags
}
_mongodb_op_counters_repl_total: {
	description: "Database replication operations by type since the mongod instance last started."
	type:        "counter"
	tags:        _mongodb_metrics_tags & {
		type: {
			description: "Counter type."
			required:    true
			examples: ["insert", "query", "update", "delete", "getmore", "command"]
		}
	}
}
_mongodb_op_counters_total: {
	description: "Database operations by type since the mongod instance last started."
	type:        "counter"
	tags:        _mongodb_metrics_tags & {
		type: {
			description: "Counter type."
			required:    true
			examples: ["insert", "query", "update", "delete", "getmore", "command"]
		}
	}
}
_mongodb_up: {
	description: "If the MongoDB server is up or not."
	type:        "gauge"
	tags:        _mongodb_metrics_tags
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
_mongodb_metrics_tags: {
	endpoint: {
		description: "The absolute path of the originating file."
		required:    true
		examples: ["mongodb://localhost:27017"]
	}
	host: {
		description: "The hostname of the MongoDB server."
		required:    true
		examples: [_values.local_host]
	}
}

// Other helpers
_disk_device: {
	description: "The disk device name."
	required:    true
	examples: ["sda", "sda1", "dm-1"]
}
_disk_counter: {
	type: "counter"
	tags: _host_metrics_tags & {
		collector: examples: ["disk"]
		device: _disk_device
	}
}
_filesystem_bytes: {
	type: "gauge"
	tags: _host_metrics_tags & {
		collector: examples: ["filesystem"]
		device: _disk_device
		filesystem: {
			description: "The name of the filesystem type."
			required:    true
			examples: ["ext4", "ntfs"]
		}
	}
}
_loadavg: {
	type: "gauge"
	tags: _host_metrics_tags & {
		collector: examples: ["loadavg"]
	}
	relevant_when: "OS is not Windows"
}
_memory_counter: {
	type: "counter"
	tags: _host_metrics_tags & {
		collector: examples: ["memory"]
	}
}
_memory_gauge: {
	type: "gauge"
	tags: _host_metrics_tags & {
		collector: examples: ["memory"]
	}
}
_memory_linux: _memory_gauge & {relevant_when: "OS is Linux"}
_memory_macos: _memory_gauge & {relevant_when: "OS is MacOS X"}
_memory_nowin: {relevant_when: "OS is not Windows"}
_network_gauge: {
	type: "gauge"
	tags: _host_metrics_tags & {
		collector: examples: ["network"]
		device: {
			description: "The network interface device name."
			required:    true
			examples: ["eth0", "enp5s3"]
		}
	}
}
_network_nomac: _network_gauge & {relevant_when: "OS is not MacOS"}

// Helpful metrics groupings
_apache_metrics: {
	apache_access_total:           _apache_access_total
	apache_connections:            _apache_connections
	apache_cpu_load:               _apache_cpu_load
	apache_cpu_seconds_total:      _apache_cpu_seconds_total
	apache_duration_seconds_total: _apache_duration_seconds_total
	apache_scoreboard:             _apache_scoreboard
	apache_sent_bytes_total:       _apache_sent_bytes_total
	apache_sent_bytes_total:       _apache_sent_bytes_total
	apache_up:                     _apache_up
	apache_uptime_seconds_total:   _apache_uptime_seconds_total
	apache_workers:                _apache_workers
}

_host_metrics: {
	host_cpu_seconds_total:                   _host_cpu_seconds_total
	host_disk_read_bytes_total:               _host_disk_read_bytes_total
	host_disk_reads_completed_total:          _host_disk_reads_completed_total
	host_disk_written_bytes_total:            _host_disk_written_bytes_total
	host_disk_writes_completed_total:         _host_disk_writes_completed_total
	host_filesystem_free_bytes:               _host_filesystem_free_bytes
	host_filesystem_total_bytes:              _host_filesystem_total_bytes
	host_filesystem_used_bytes:               _host_filesystem_used_bytes
	host_load1:                               _host_load1
	host_load5:                               _host_load5
	host_load15:                              _host_load15
	host_memory_active_bytes:                 _host_memory_active_bytes
	host_memory_available_bytes:              _host_memory_available_bytes
	host_memory_buffers_bytes:                _host_memory_buffers_bytes
	host_memory_cached_bytes:                 _host_memory_cached_bytes
	host_memory_free_bytes:                   _host_memory_free_bytes
	host_memory_inactive_bytes:               _host_memory_inactive_bytes
	host_memory_shared_bytes:                 _host_memory_shared_bytes
	host_memory_swap_free_bytes:              _host_memory_swap_free_bytes
	host_memory_swapped_in_bytes_total:       _host_memory_swapped_in_bytes_total
	host_memory_swapped_out_bytes_total:      _host_memory_swapped_out_bytes_total
	host_memory_swap_total_bytes:             _host_memory_swap_total_bytes
	host_memory_swap_used_bytes:              _host_memory_swap_used_bytes
	host_memory_total_bytes:                  _host_memory_total_bytes
	host_memory_used_bytes:                   _host_memory_used_bytes
	host_memory_wired_bytes:                  _host_memory_wired_bytes
	host_network_receive_bytes_total:         _host_network_receive_bytes_total
	host_network_receive_errs_total:          _host_network_receive_errs_total
	host_network_receive_packets_total:       _host_network_receive_packets_total
	host_network_transmit_bytes_total:        _host_network_transmit_bytes_total
	host_network_transmit_errs_total:         _host_network_transmit_errs_total
	host_network_transmit_packets_drop_total: _host_network_transmit_packets_drop_total
	host_network_transmit_packets_total:      _host_network_transmit_packets_total
}

_mongodb_metrics: {
	mongodb_assets_total:                                                _mongodb_assets_total
	mongodb_bson_parse_error_total:                                      _mongodb_bson_parse_error_total
	mongodb_connections:                                                 _mongodb_connections
	mongodb_extra_info_heap_usage_bytes:                                 _mongodb_extra_info_heap_usage_bytes
	mongodb_extra_info_page_faults:                                      _mongodb_extra_info_page_faults
	mongodb_instance_local_time:                                         _mongodb_instance_local_time
	mongodb_instance_uptime_estimate_seconds_total:                      _mongodb_instance_uptime_estimate_seconds_total
	mongodb_instance_uptime_seconds_total:                               _mongodb_instance_uptime_seconds_total
	mongodb_memory:                                                      _mongodb_memory
	mongodb_mongod_global_lock_active_clients:                           _mongodb_mongod_global_lock_active_clients
	mongodb_mongod_global_lock_current_queue:                            _mongodb_mongod_global_lock_current_queue
	mongodb_mongod_locks_time_acquiring_global_seconds_total:            _mongodb_mongod_locks_time_acquiring_global_seconds_total
	mongodb_mongod_metrics_cursor_open:                                  _mongodb_mongod_metrics_cursor_open
	mongodb_mongod_metrics_cursor_timed_out_total:                       _mongodb_mongod_metrics_cursor_timed_out_total
	mongodb_mongod_metrics_document_total:                               _mongodb_mongod_metrics_document_total
	mongodb_mongod_metrics_get_last_error_wtime_num:                     _mongodb_mongod_metrics_get_last_error_wtime_num
	mongodb_mongod_metrics_get_last_error_wtime_seconds_total:           _mongodb_mongod_metrics_get_last_error_wtime_seconds_total
	mongodb_mongod_metrics_get_last_error_wtimeouts_total:               _mongodb_mongod_metrics_get_last_error_wtimeouts_total
	mongodb_mongod_metrics_operation_total:                              _mongodb_mongod_metrics_operation_total
	mongodb_mongod_metrics_query_executor_total:                         _mongodb_mongod_metrics_query_executor_total
	mongodb_mongod_metrics_record_moves_total:                           _mongodb_mongod_metrics_record_moves_total
	mongodb_mongod_metrics_repl_apply_batches_num_total:                 _mongodb_mongod_metrics_repl_apply_batches_num_total
	mongodb_mongod_metrics_repl_apply_batches_seconds_total:             _mongodb_mongod_metrics_repl_apply_batches_seconds_total
	mongodb_mongod_metrics_repl_apply_ops_total:                         _mongodb_mongod_metrics_repl_apply_ops_total
	mongodb_mongod_metrics_repl_buffer_count:                            _mongodb_mongod_metrics_repl_buffer_count
	mongodb_mongod_metrics_repl_buffer_max_size_bytes_total:             _mongodb_mongod_metrics_repl_buffer_max_size_bytes_total
	mongodb_mongod_metrics_repl_buffer_size_bytes:                       _mongodb_mongod_metrics_repl_buffer_size_bytes
	mongodb_mongod_metrics_repl_executor_queue:                          _mongodb_mongod_metrics_repl_executor_queue
	mongodb_mongod_metrics_repl_executor_unsignaled_events:              _mongodb_mongod_metrics_repl_executor_unsignaled_events
	mongodb_mongod_metrics_repl_network_bytes_total:                     _mongodb_mongod_metrics_repl_network_bytes_total
	mongodb_mongod_metrics_repl_network_getmores_num_total:              _mongodb_mongod_metrics_repl_network_getmores_num_total
	mongodb_mongod_metrics_repl_network_getmores_seconds_total:          _mongodb_mongod_metrics_repl_network_getmores_seconds_total
	mongodb_mongod_metrics_repl_network_ops_total:                       _mongodb_mongod_metrics_repl_network_ops_total
	mongodb_mongod_metrics_repl_network_readers_created_total:           _mongodb_mongod_metrics_repl_network_readers_created_total
	mongodb_mongod_metrics_ttl_deleted_documents_total:                  _mongodb_mongod_metrics_ttl_deleted_documents_total
	mongodb_mongod_metrics_ttl_passes_total:                             _mongodb_mongod_metrics_ttl_passes_total
	mongodb_mongod_op_latencies_histogram:                               _mongodb_mongod_op_latencies_histogram
	mongodb_mongod_op_latencies_latency:                                 _mongodb_mongod_op_latencies_latency
	mongodb_mongod_op_latencies_ops_total:                               _mongodb_mongod_op_latencies_ops_total
	mongodb_mongod_storage_engine:                                       _mongodb_mongod_storage_engine
	mongodb_mongod_wiredtiger_blockmanager_blocks_total:                 _mongodb_mongod_wiredtiger_blockmanager_blocks_total
	mongodb_mongod_wiredtiger_blockmanager_bytes_total:                  _mongodb_mongod_wiredtiger_blockmanager_bytes_total
	mongodb_mongod_wiredtiger_cache_bytes:                               _mongodb_mongod_wiredtiger_cache_bytes
	mongodb_mongod_wiredtiger_cache_bytes_total:                         _mongodb_mongod_wiredtiger_cache_bytes_total
	mongodb_mongod_wiredtiger_cache_evicted_total:                       _mongodb_mongod_wiredtiger_cache_evicted_total
	mongodb_mongod_wiredtiger_cache_max_bytes:                           _mongodb_mongod_wiredtiger_cache_max_bytes
	mongodb_mongod_wiredtiger_cache_overhead_percent:                    _mongodb_mongod_wiredtiger_cache_overhead_percent
	mongodb_mongod_wiredtiger_cache_pages:                               _mongodb_mongod_wiredtiger_cache_pages
	mongodb_mongod_wiredtiger_cache_pages_total:                         _mongodb_mongod_wiredtiger_cache_pages_total
	mongodb_mongod_wiredtiger_concurrent_transactions_available_tickets: _mongodb_mongod_wiredtiger_concurrent_transactions_available_tickets
	mongodb_mongod_wiredtiger_concurrent_transactions_out_tickets:       _mongodb_mongod_wiredtiger_concurrent_transactions_out_tickets
	mongodb_mongod_wiredtiger_concurrent_transactions_total_tickets:     _mongodb_mongod_wiredtiger_concurrent_transactions_total_tickets
	mongodb_mongod_wiredtiger_log_bytes_total:                           _mongodb_mongod_wiredtiger_log_bytes_total
	mongodb_mongod_wiredtiger_log_operations_total:                      _mongodb_mongod_wiredtiger_log_operations_total
	mongodb_mongod_wiredtiger_log_records_scanned_total:                 _mongodb_mongod_wiredtiger_log_records_scanned_total
	mongodb_mongod_wiredtiger_log_records_total:                         _mongodb_mongod_wiredtiger_log_records_total
	mongodb_mongod_wiredtiger_session_open_sessions:                     _mongodb_mongod_wiredtiger_session_open_sessions
	mongodb_mongod_wiredtiger_transactions_checkpoint_seconds:           _mongodb_mongod_wiredtiger_transactions_checkpoint_seconds
	mongodb_mongod_wiredtiger_transactions_checkpoint_seconds_total:     _mongodb_mongod_wiredtiger_transactions_checkpoint_seconds_total
	mongodb_mongod_wiredtiger_transactions_running_checkpoints:          _mongodb_mongod_wiredtiger_transactions_running_checkpoints
	mongodb_mongod_wiredtiger_transactions_total:                        _mongodb_mongod_wiredtiger_transactions_total
	mongodb_network_bytes_total:                                         _mongodb_network_bytes_total
	mongodb_network_metrics_num_requests_total:                          _mongodb_network_metrics_num_requests_total
	mongodb_op_counters_repl_total:                                      _mongodb_op_counters_repl_total
	mongodb_op_counters_total:                                           _mongodb_op_counters_total
	mongodb_up:                                                          _mongodb_up
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
