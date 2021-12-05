package metadata

administration: topologies: stream_based: {
	title: "Stream based"
	order: 3
	description: """
		The most durable and elastic topology. This topology is typically adopted for very large streams with teams that
		are familiar with running a stream-based service such as Kafka.
		"""

	pros: [
		{
			title: "Most durable and reliable"
			description: """
				Stream services, like Kafka, are designed for high durability and reliability, replicating data across
				multiple nodes.
				"""
		},
		{
			title: "Most efficient"
			description: """
				Vector agents are doing less, making them more efficient, and Vector services do not have to worry about
				durability, which can be tuned towards performance.
				"""
		},
		{
			title: "Ability to re-stream"
			description: """
				Re-stream your data depending on your stream's retention period.
				"""
		},
		{
			title: "Cleaner separation of responsibilities"
			description: """
				Vector is used solely as a routing layer and is not responsible for durability. Durability is delegated
				to a purpose-built service that you can switch and evolve over time.
				"""
		},
	]

	cons: [
		{
			title: "Increased management overhead"
			description: """
				Managing a stream service, such as Kafka, is a complex endeavor and generally requires an experienced
				team to setup and manage properly.
				"""
		},
		{
			title: "More complex"
			description: """
				This topology is complex and requires a deeper understand of managing production-grade streams.
				"""
		},
		{
			title: "More expensive"
			description: """
				In addition the management cost, the added stream cluster will require more resources which will
				increase operational cost.
				"""
		},
	]
}
