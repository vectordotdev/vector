remap: features: stateless: {
	title:       "Stateless"
	description: """
		VRL programs are stateless, operating on a single event at a time. This makes VRL programs simple, fast, and
		safe. Operations involving state across events, such as [deduplication](\(urls.vector_dedupe_transform)), are
		delegated to other Vector transforms designed specifically for stateful operations.
		"""

	principles: {
		performance: true
		safety:      true
	}
}
