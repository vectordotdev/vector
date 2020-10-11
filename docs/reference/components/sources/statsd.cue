package metadata

components: sources: statsd: {
	title:             "StatsD"
	short_description: "Ingests data through the [StatsD UDP protocol][urls.statsd_udp_protocol] and outputs metric events."
	long_description:  "[StatsD][urls.statsd] is a standard and, by extension, a set of tools that can be used to send, collect, and aggregate custom metrics from any application. Originally, StatsD referred to a daemon written by [Etsy][urls.etsy] in Node."

	classes: {
		commonly_used: false
		deployment_roles: ["aggregator"]
		egress_method: "stream"
		function:      "receive"
	}

	features: {
		checkpoint: enabled: false
		multiline: enabled:  false
		tls: enabled:        false
	}

	statuses: {
		delivery:    "best_effort"
		development: "stable"
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: [
			"""
				This component exposes a configured port. You must ensure your network allows access to this port.
				""",
		]
		warnings: []
		notices: []
	}

	configuration: {
		address: {
			description: "UDP socket address to bind to."
			required:    true
			warnings: []
			type: string: {
				examples: ["127.0.0.1:8126"]
			}
		}
	}

	output: metrics: {
		counter:   output._passthrough_counter
		gauge:     output._passthrough_gauge
		histogram: output._passthrough_histogram
		set:       output._passthrough_set
	}

	how_it_works: {
		timestamps: {
			title: "Timestamps"
			body: #"""
				StatsD protocol does not provide support for sending metric timestamps. You'll
				notice that each parsed metric is assigned a `null` timestamp, which is a
				special value which means "a real time metric", i.e. not a historical one. Normally such
				`null` timestamps will be substituted by current time by downstream sinks or
				3rd party services during sending/ingestion. See the [metric][docs.data-model.metric]
				data model page for more info.
				"""#
		}
	}
}
