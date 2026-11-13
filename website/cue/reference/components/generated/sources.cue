package metadata

generated: components: sources: configuration: {
	graph: {
		description: """
			Extra graph configuration

			Configure output for component when generated with graph command
			"""
		required: false
		type:     _schemaDefinitions["vector::config::dot_graph::GraphConfig"]
	}
	measure_cpu_usage: {
		description: """
			Enable CPU usage metrics for this source.

			When set to `true`, each poll of the source task, and of the tasks it spawns (such as
			per-connection decoding), is timed using the OS thread CPU clock and the accumulated
			nanoseconds are reported as the `component_cpu_usage_ns_total` counter, tagged with
			`component_id`, `component_kind`, and `component_type`.

			Defaults to `false`. Enable only for sources where CPU attribution is needed, as it adds a
			`clock_gettime` call on every future poll.

			CPU spent on threads or tasks that a third-party library manages on its own is not
			included. Notably, the `vector` and `opentelemetry` sources decode gRPC requests on
			connection tasks spawned by `tonic`, and the `kafka` source decompresses batches on
			librdkafka threads.

			On Windows, `component_cpu_usage_ns_total` is a rough estimate: thread CPU time advances
			only at the scheduler clock tick (typically 15.6 ms), so short polls are counted either as
			zero or as a whole tick.
			"""
		required: false
		type: bool: default: false
	}
	proxy: {
		description: """
			Proxy configuration.

			Configure to proxy traffic through an HTTP(S) proxy when making external requests.

			Similar to common proxy configuration convention, you can set different proxies
			to use based on the type of traffic being proxied. You can also set specific hosts that
			should not be proxied.
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::proxy::ProxyConfig"]
	}
}
