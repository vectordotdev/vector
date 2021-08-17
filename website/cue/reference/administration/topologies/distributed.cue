package metadata

administration: topologies: distributed: {
	title: "Distributed"
	order: 1
	description: """
		The simplest topology. In a distributed setup, Vector communicates directly with your downstream services from
		your client nodes.
		"""

	pros: [
		{
			title:       "Simple"
			description: "Fewer moving parts"
		},
		{
			title:       "Elastic"
			description: "Easily scales with your app. Resources grow as you scale."
		},
	]

	cons: [
		{
			title: "Less efficient"
			description: """
				Depending on the complexity of your pipelines, this will use more local resources, which could disrupt
				the performance of other applications on the same host.
				"""
		},
		{
			title: "Less durable"
			description: """
				Because data is buffered on the host it is more likely you'll lose buffered data in the event of an
				unrecoverable crash. Often times this is the most important and useful data.
				"""
		},
		{
			title: "More downstream stress"
			description: """
				Downstream services will receive more requests with smaller payloads that could potentially disrupt
				stability of these services.
				"""
		},
		{
			title: "Reduced downstream stability"
			description: """
				You risk overloading downstream services if you need to scale up quickly or exceed the capacity a
				downstream service can handle.
				"""
		},
		{
			title: "Lacks multi-host context"
			description: """
				Lacks awareness of other hosts and eliminates the ability to perform operations across hosts, such as
				reducing logs to global metrics. This is typically a concern for very large deployments where individual
				host metrics are less useful.
				"""
		},
	]
}
