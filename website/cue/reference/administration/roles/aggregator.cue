package metadata

administration: roles: aggregator: {
	title: "Aggregator"
	description: """
		The Aggregator role is designed for central processing, collecting data from
		multiple upstream sources and performing cross-host aggregation and analysis.

		For Vector, this role should be reserved for exactly that: cross-host aggregation
		and analysis. Vector is unique in the fact that it can serve both as an Agent
		and an Aggregator. This makes it possible to distribute processing along the edge
		(recommended). We highly recommend pushing processing to the edge when possible
		since it is more efficient and easier to manage.
		"""
}
