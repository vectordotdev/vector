package metadata

components: transforms: remove_tags: {
	title: "Remove Tags"

	description: """
		Removes one or more metric tags.
		"""

	classes: {
		commonly_used: false
		development:   "deprecated"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		shape: {}
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
		warnings: [
			"""
			This component has been deprecated in favor of the new [`remap` transform's `del`
			function](\(urls.vector_remap_transform)#del). The `remap` transform provides a simple
			syntax for robust data transformation. Let us know what you think!
			""",
		]
		notices: []
	}

	configuration: {
		tags: {
			description: "The tag names to drop."
			required:    true
			warnings: []
			type: array: items: type: string: {
				examples: ["tag1", "tag2"]
				syntax: "literal"
			}
		}
	}

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
	}
}
