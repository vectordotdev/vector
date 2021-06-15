package metadata

administration: topologies: centralized: {
	title: "Centralized"
	order: 2
	description: """
		A good balance of simplicity, stability, and control. For manu use cases, a centralized deployment topology is
		a good compromise between the [distributed](#distributed) and [stream-based](#stream-based) topologies, as it
		offers many of the advantages of a stream-based topology, such as a clean separation of responsibilities,
		without the management overheard incurred by a stream-based setup, which often involves using Vector in
		conjunction with a system like [Apache Kafka](\(urls.kafka)) or [Apache Pulsar](\(urls.pulsar)).
		"""

	pros: [
		{
			title: "More efficient"
			description: """
				Centralized topologies are typically more efficient for client nodes and downstream services. Vector
				[agents](\(urls.vector_agent_role)) do less work and thus use fewer resources. In addition, in this
				topology the centralized Vector service buffers data, provides better compression, and sends optimized
				requests downstream.
				"""
		},
		{
			title: "More reliable"
			description: """
				Vector protects downstream services from volume spikes by buffering and flushing data at smoothed-out
				intervals.
				"""
		},
		{
			title: "Has multi-host context"
			description: """
				Because your data is centralized, you can perform operations across hosts, such as reducing logs to
				global metrics. This can be advantageous for large deployments in which individual host metrics don't
				provide much useful information.
				"""
		}
	]

	cons: [
		{
			title: "More complex"
			description: """
				A centralized topology has more moving parts, as you need to run Vector in both the
				[agent](\(urls.vector_agent_role)) and [aggregator](\(urls.vector_aggregator_role)) roles.
				"""
		},
		{
			title: "Less durable"
			description: """
				[Agent](\(urls.vector_agent_role)) nodes are designed to get data off of the machine as quickly as
				possible. While this is fine for some use cases, it does bear the potential for data loss since the
				central Vector service could go down and thus lose buffered data. If this type of outage is unacceptable
				for your requirements, we recommend running a [stream-based](#stream-based) topology.
				"""
		},
	]
}
