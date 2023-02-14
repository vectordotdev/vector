package metadata

components: sinks: hdfs: {
	title: "HDFS"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"

		development:   "beta"
		egress_method: "stream"
		service_providers: []
		stateful: false
	}

	features: {
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			compression: {
				enabled: true
				default: "none"
				algorithms: ["none", "gzip", "zstd"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					framing: true
					enum: ["json", "text"]
				}
			}
			request: enabled: false
			tls: enabled:     false
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		root: {
			description: "Root path of hdfs services."
			required:    true
			type: string: {}
		}
		prefix: {
			description: """
				A prefix to apply to all keys.

				Prefixes are useful for partitioning objects, such as by creating an blob key that stores blobs under a particular "directory". If using a prefix for this purpose, it must end in `/` to act as a directory path. A trailing `/` is **not**
			"""
			required:    true
			type: string: syntax: "template"
		}
		name_node: {
			description: """
				An HDFS cluster consists of a single NameNode, a master server that manages the file system namespace and regulates access to files by clients.

				For example:

				- `default`: visiting local fs.
				- `http://172.16.80.2:8090` visiting name node at `172.16.80.2`

				For more information: [HDFS Architecture](https://hadoop.apache.org/docs/r3.3.4/hadoop-project-dist/hadoop-hdfs/HdfsDesign.html#NameNode_and_DataNodes)
			"""
			required:    true
			type: string: {}
		}
	}

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	telemetry: metrics: {
		component_sent_bytes_total:       components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
	}
}
