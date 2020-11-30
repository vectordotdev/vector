package metadata

installation: roles: {
	agent: {
		title: "Agent"
		sub_roles: {
			daemon: {
				title: "Daemon"
				description: """
					The daemon role is designed to collect _all_ data on a single host. This is
					the recommended role for data collection since it the most efficient use
					of host resources. Vector implements a directed acyclic graph topology model,
					enabling the collection and processing from mutliple services.
					"""
			}
			sidecar: {
				title: "Sidecar"
				description: """
					The sidecar role couples Vector with each service, focused on data collection
					for that individual service only. While the deamon role is recommended, the
					sidecar role is beneficial when you want to shift reponsibility of data
					collection to the service owner. And, in some cases, it can be simpler to
					manage.
					"""
			}
		}
	}
	aggregator: {
		title: "Aggregator"
		description: """
			The aggregator role is designed for central processing, collecting data from
			multiple upstream sources and performing cross-host aggregation and analysis.

			For Vector, this role should be reserved for exactly that: cross-host aggregation
			and analysis. Vector is unique in the fact that it can serve both as an agent
			and aggregator. This makes it possible to distribute processing along the edge
			(recommended). We highly recommend pushing processing to the edge when possible
			since it is more efficient and easier to manage.
			"""
	}
}
