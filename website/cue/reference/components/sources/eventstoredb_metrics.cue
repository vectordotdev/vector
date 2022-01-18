package metadata

components: sources: eventstoredb_metrics: {
	title: "EventStoreDB Metrics"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "sidecar"]
		development:   "beta"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		collect: {
			checkpoint: enabled: false
			from: {
				service: services.eventstoredb_stats_client

				interface: socket: {
					api: {
						title: "EventStoreDB"
						url:   urls.eventstoredb_stats_client
					}
					direction: "outgoing"
					protocols: ["http"]
					ssl: "optional"
				}
			}
			proxy: enabled: true
		}
		multiline: enabled: false
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
		endpoints: {
			common:      true
			description: "Endpoints to scrape stats from."
			required:    false
			type: string: {
				examples: ["https://localhost:2113/stats"]
				default: "https://localhost:2113/stats"
			}
		}
		scrape_interval_secs: {
			common:      true
			description: "The interval between scrapes, in seconds."
			required:    false
			type: uint: {
				default: 15
				unit:    "seconds"
			}
		}
		default_namespace: {
			common:      false
			description: "The namespace used otherwise will be defaulted to eventstoredb."
			required:    false
			type: string: {
				examples: ["app-123-eventstoredb"]
				default: "eventstoredb"
			}
		}
	}

	output: metrics: {
		_eventstoredb_metrics_tags: {
			id: {
				description: "The process id of the EventStoreDB node."
				required:    true
				examples: ["1234567"]
			}
			path: {
				description: "Location of the EventStoreDB node data directory."
				required:    false
				examples: ["/foo/bar/baz"]
			}
		}

		process_memory_used_bytes: {
			description:       "The number of bytes of main memory used by the EventStoreDB node."
			type:              "gauge"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
		disk_read_bytes_total: {
			description:       "The accumulated number of bytes read in from disk."
			type:              "counter"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
		disk_written_bytes_total: {
			description:       "The accumulated number of bytes written out to disk."
			type:              "counter"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
		disk_read_ops_total: {
			description:       "The accumulated number of read IOPS."
			type:              "counter"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
		disk_write_ops_total: {
			description:       "The accumulated number of write IOPS."
			type:              "counter"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
		memory_free_bytes: {
			description:       "The number of bytes of main memory not used."
			type:              "gauge"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
		disk_total_bytes: {
			description:       "The total number of bytes in disk."
			type:              "gauge"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
		disk_free_bytes: {
			description:       "The number of bytes free on disk."
			type:              "gauge"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
		disk_used_bytes: {
			description:       "The number of bytes used on disk."
			type:              "gauge"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
	}
	telemetry: metrics: {
		events_in_total:                      components.sources.internal_metrics.output.metrics.events_in_total
		http_request_errors_total:            components.sources.internal_metrics.output.metrics.http_request_errors_total
		parse_errors_total:                   components.sources.internal_metrics.output.metrics.parse_errors_total
		processed_bytes_total:                components.sources.internal_metrics.output.metrics.processed_bytes_total
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
	}
}
