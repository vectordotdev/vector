package metadata

generated: components: sources: okta: configuration: {
	domain: {
		description: "The Okta subdomain to scrape"
		required:    true
		type: string: examples: ["foo.okta.com"]
	}
	scrape_interval_secs: {
		description: """
			The interval between scrapes. Requests are run concurrently so if a scrape takes longer
			than the interval, a new scrape will be started. This can take extra resources, set the timeout
			to a value lower than the scrape interval to prevent this from happening.
			"""
		required: false
		type: uint: {
			default: 15
			unit:    "seconds"
		}
	}
	scrape_timeout_secs: {
		description: "The timeout for each scrape request."
		required:    false
		type: float: {
			default: 5.0
			unit:    "seconds"
		}
	}
	since: {
		description: """
			The time to look back for logs. This is used to determine the start time of the first request
			(that is, the earliest log to fetch)
			"""
		required: false
		type: uint: {}
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
	token: {
		description: "API token for authentication"
		required:    true
		type: string: examples: ["00xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"]
	}
}
