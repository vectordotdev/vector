package metadata

installation: _interfaces: [string]: role_implementations: _vector_aggregator: {
	variables: config: {
		sources: {
			vector: type:           components.sources.vector.type
			internal_metrics: type: components.sources.internal_metrics.type
		}
	}
	description: #"""
				The aggregator role is designed to receive and
				process data from multiple upstream agents.
				Typically these are other Vector agents, but it
				could be anything, including non-Vector agents.
				By default, we recommend the [`vector` source](\#(urls.vector_source))
				since it supports all data types, but it is
				recommended to adjust your pipeline as necessary
				using Vector's [sources](\#(urls.vector_sources)),
				[transforms](\#(urls.vector_transforms)), and
				[sinks](\#(urls.vector_sinks)).
				"""#
	title:       "Aggregator"
}
