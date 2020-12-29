package metadata

services: vector: {
	name:     "Vector"
	thing:    "a \(name) instance"
	url:      urls.vector_docs
	versions: ">= 0.11.0"

	connect_to: {
		splunk: logs: {
			setup: [
				{
					title: "Create a Splunk HEC endpoint"
					description: """
						Follow the Splunk HEC setup docs to create a Splunk HEC endpoint.
						"""
					detour: url: urls.splunk_hec_setup
				},
				{
					title: "Configure Vector"
					description: """
						Splunk will provide you with a host and token. Copy those
						values to the `host` and `token` options.
						"""
					vector: configure: sinks: splunk_hec: {
						type:  "splunk_hec"
						host:  "<host>"
						token: "<token>"
					}
				},
			]
		}
	}
}
