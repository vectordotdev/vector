remap: features: stateless: {
	title:       "Stateless"
	description: """
		VRL programs are stateless, operating on a single event at a time. This limits the complexity of VRL programs,
		making them simple, and contributing to VRL's performance and safety principles. Stateful operations, such as
		[deduplication](\(urls.vector_dedupe_transform)), are delegated to other Vector transforms designed
		specifically for the stateful operation, guardrails and all.
		"""

	principles: {
		performance: true
		safety:      true
	}
}
