package metadata

base: components: sources: apache_metrics: configuration: {
	endpoints: {
		description: "The list of `mod_status` endpoints to scrape metrics from."
		required:    true
		type: array: items: type: string: syntax: "literal"
	}
	namespace: {
		description: """
			The namespace of the metric.

			Disabled if empty.
			"""
		required: false
		type: string: {
			default: "apache"
			syntax:  "literal"
		}
	}
	scrape_interval_secs: {
		description: "The interval between scrapes, in seconds."
		required:    false
		type: uint: default: 15
	}
}
