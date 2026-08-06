package metadata

administration: interfaces: [string]: role_implementations: _journald_agent: {
	variables: config: {
		sources: {
			logs: type:             components.sources.journald.type
			host_metrics: type:     components.sources.host_metrics.type
			internal_metrics: type: components.sources.internal_metrics.type
		}
	}
	description: #"""
		The agent role is designed to collect all data on a single host. Vector runs as a background
		process and interfaces with a host-level APIs for data collection. By default, Vector will
		collect logs from [Journald](\#(urls.journald)) via Vector's
		[`journald` source](\#(urls.vector_journald_source)) and metrics via the [`host_metrics`
		source](\#(urls.vector_host_metrics_source)), but we recommend that you adjust your pipeline
		as necessary using Vector's [sources](\#(urls.vector_sources)),
		[transforms](\#(urls.vector_transforms)), and [sinks](\#(urls.vector_sinks)).
		"""#
	title:       "Agent"
}
