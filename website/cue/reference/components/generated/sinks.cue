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
		type:        _schemaDefinitions["derived::d1a2a37da28b48074a6aa38c"]
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
