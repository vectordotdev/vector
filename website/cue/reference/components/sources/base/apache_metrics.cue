package metadata

base: components: sources: apache_metrics: configuration: {
	endpoints: {
		description: "The list of `mod_status` endpoints to scrape metrics from."
		required:    true
		type: array: items: type: string: examples: ["http://localhost:8080/server-status/?auto"]
	}
	namespace: {
		description: """
			The namespace of the metric.

			Disabled if empty.
			"""
		required: false
		type: string: default: "apache"
	}
	scrape_interval_secs: {
		description: "The interval between scrapes."
		required:    false
		type: uint: {
			default: 15
			unit:    "seconds"
		}
	}
}
