remap2: features: stateless: {
	title:       "Stateless"
	description: """
		VRL is stateless, operating on a single Vector event at a time. This limits the complexity of VRL programs,
		making them simple, and contributing to VRL's performance and safety principle. Stateful operations, such as
		[deduplication](\(urls.vector_dedupe_transform)), are delegated to other Vector transforms designed
		specifically for the stateful operation, guardrails and all.
		"""
}
