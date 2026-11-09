package metadata

generated: components: sinks: configuration: {
	buffer: {
		description: """
			Configures the buffering behavior for this sink.

			More information about the individual buffer types, and buffer behavior, can be found in the
			[Buffering Model][buffering_model] section.

			[buffering_model]: /docs/architecture/buffering-model/
			"""
		required: false
		type:     _schemaDefinitions["derived::vector_buffers::config::BufferType::6c60e9549d874ce4350c077a"]
	}
	graph: {
		description: """
			Extra graph configuration

			Configure output for component when generated with graph command
			"""
		required: false
		type:     _schemaDefinitions["vector::config::dot_graph::GraphConfig"]
	}
	healthcheck: {
		description: "Healthcheck configuration."
		required:    false
		type:        _schemaDefinitions["derived::7ad58e4d9af48ec29d981195"]
	}
	inputs: {
		description: """
			A list of upstream [source][sources] or [transform][transforms] IDs.

			Wildcards (`*`) are supported.

			See [configuration][configuration] for more info.

			[sources]: https://vector.dev/docs/reference/configuration/sources/
			[transforms]: https://vector.dev/docs/reference/configuration/transforms/
			[configuration]: https://vector.dev/docs/reference/configuration/
			"""
		required: true
		type: array: items: type: string: examples: ["my-source-or-transform-id", "prefix-*"]
	}
	measure_cpu_usage: {
		description: """
			Enable CPU usage metrics for this sink.

			When set to `true`, each poll of the sink task, and of the tasks it spawns (such as request
			building or HTTP connections), is timed using the OS thread CPU clock and the accumulated
			nanoseconds are reported as the `component_cpu_usage_ns_total` counter, tagged with
			`component_id`, `component_kind`, and `component_type`.

			Defaults to `false`. Enable only for sinks where CPU attribution is needed, as it adds a
			`clock_gettime` call on every future poll.

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
