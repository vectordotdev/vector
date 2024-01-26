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
		acknowledgements: false
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

	configuration: base.components.sources.eventstoredb_metrics.configuration

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
}
