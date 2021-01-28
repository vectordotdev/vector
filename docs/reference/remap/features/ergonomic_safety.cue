remap: features: ergonomic_safety: {
	title: "Ergonomic safety"
	description: """
		VRL is ergonomically designed to be safe, preventing the production of slow and unreliable programs. VRL is
		designed to execute in the hot-path, and therefore includes intentional limitations, preventing common
		foot-guns that often plague observability pipelines. Conversely, deep, purpose-built observability features
		prevent the need for unsafe low-level constructs present in generic languages.
		"""

	principles: {
		performance: true
		safety:      true
	}

	characteristics: {
		limitations: {
			title: "Limitations"
			description: """
				VRL is intentionally designed with limitations to prevent foot-guns that commonly plague observability
				pipelines. To name a few:

				1. Lack of custom classes, modules, and functions.
				2. Lack of recursion.
				3. Lack of direct access to low-level system resources that require caching, such as the network or disk.
				4. Lack of state.

				If an observability use cases requires any of these, they will be pushed into purpose-built functions
				that are carefully designed for performance and safety.
				"""
		}
		purpose_built: {
			title: "Purpose-built"
			description: """
				Conversely from limitations, VRL goes deep on purpose-built observability use cases, avoiding the
				need for unsafe low-level constructs. Functions like `parse_syslog` and	`to_hive_partition` make
				otherwise complex tasks simple.
				"""
		}
	}
}
