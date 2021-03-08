package metadata

components: sources: eventstoredb_metrics: {
	title: "EventStoreDB Metrics"
	alias: "eventstoredb"

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
		}
		multiline: enabled: false
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
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
			warnings: []
			type: string: {
				examples: ["https://localhost:2113/"]
				default: "https://localhost:2113/"
				syntax:  "literal"
			}
		}
		scrape_interval_secs: {
			common:      true
			description: "The interval between scrapes, in seconds."
			required:    false
			warnings: []
			type: uint: {
				default: 3
				unit:    "seconds"
			}
		}
		default_namespace: {
			common:      false
			description: "The namespace used otherwise will be defaulted to eventstoredb."
			required:    false
			warnings: []
			type: string: {
				examples: ["app-123-eventstoredb"]
				default: "eventstoredb"
				syntax:  "literal"
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

		memory_usage: {
			description:       "Amount of used memory on the machine."
			type:              "gauge"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}

		disk_io_read_bytes: {
			description:       "Number of bytes read from the drive."
			type:              "counter"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
		disk_io_written_bytes: {
			description:       "Number of bytes written to the drive."
			type:              "counter"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
		disk_io_read_ops: {
			description:       "Number of read IOPS from the drive."
			type:              "counter"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
		disk_io_write_ops: {
			description:       "Number of write IOPS to the drive."
			type:              "counter"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
		free_memory: {
			description:       "Amount of free memory on the machine."
			type:              "gauge"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
		drive_total_bytes: {
			description:       "Capacity of the drive in bytes."
			type:              "gauge"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
		drive_available_bytes: {
			description:       "Amount of available storage in bytes."
			type:              "gauge"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
		drive_used_bytes: {
			description:       "Amount of used storage in bytes."
			type:              "gauge"
			default_namespace: "eventstoredb"
			tags:              _eventstoredb_metrics_tags
		}
	}
	telemetry: metrics: {
		events_in_total:           components.sources.internal_metrics.output.metrics.events_in_total
		http_request_errors_total: components.sources.internal_metrics.output.metrics.http_request_errors_total
		parse_errors_total:        components.sources.internal_metrics.output.metrics.parse_errors_total
		processed_bytes_total:     components.sources.internal_metrics.output.metrics.processed_bytes_total
	}
}
