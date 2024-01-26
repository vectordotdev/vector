package metadata

base: components: sources: mongodb_metrics: configuration: {
	endpoints: {
		description: """
			A list of MongoDB instances to scrape.

			Each endpoint must be in the [Connection String URI Format](https://www.mongodb.com/docs/manual/reference/connection-string/).
			"""
		required: true
		type: array: items: type: string: examples: ["mongodb://localhost:27017"]
	}
	namespace: {
		description: """
			Overrides the default namespace for the metrics emitted by the source.

			If set to an empty string, no namespace is added to the metrics.

			By default, `mongodb` is used.
			"""
		required: false
		type: string: default: "mongodb"
	}
	scrape_interval_secs: {
		description: "The interval between scrapes, in seconds."
		required:    false
		type: uint: {
			default: 15
			unit:    "seconds"
		}
	}
}
