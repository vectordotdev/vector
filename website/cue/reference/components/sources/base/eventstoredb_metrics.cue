package metadata

base: components: sources: eventstoredb_metrics: configuration: {
	default_namespace: {
		description: """
			Overrides the default namespace for the metrics emitted by the source.

			By default, `eventstoredb` is used.
			"""
		required: false
		type: string: syntax: "literal"
	}
	endpoint: {
		description: "Endpoints to scrape stats from."
		required:    false
		type: string: {
			default: "https://localhost:2113/stats"
			syntax:  "literal"
		}
	}
	scrape_interval_secs: {
		description: "The interval between scrapes, in seconds."
		required:    false
		type: uint: default: 15
	}
}
