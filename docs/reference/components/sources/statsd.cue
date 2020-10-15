package metadata

components: sources: statsd: {
	_port: 8126

	title:             "StatsD"
	short_description: "Ingests data through the [StatsD UDP protocol][urls.statsd_udp_protocol] and outputs metric events."
	long_description:  "[StatsD][urls.statsd] is a standard and, by extension, a set of tools that can be used to send, collect, and aggregate custom metrics from any application. Originally, StatsD referred to a daemon written by [Etsy][urls.etsy] in Node."

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		deployment_roles: ["aggregator"]
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		multiline: enabled: false
		receive: {
			from: {
				title:    "StatsD Client"
				url:      urls.statsd
				versions: null

				interface: socket: {
					api: {
						title: "StatsD"
						url:   urls.statsd_udp_protocol
					}
					port: _port
					protocols: ["udp"]
					ssl: "optional"
				}
			}

			tls: enabled: false
		}
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

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		address: {
			description: "UDP socket address to bind to."
			required:    true
			warnings: []
			type: string: {
				examples: ["127.0.0.1:\(_port)"]
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
				StatsD protocol does not provide support for sending metric
				timestamps. You'll notice that each parsed metric is assigned a
				`null` timestamp, which is a special value which means "a real
				time metric", i.e. not a historical one. Normally such `null`
				timestamps will be substituted by current time by downstream
				sinks or 3rd party services during sending/ingestion. See the
				[metric][docs.data-model.metric] data model page for more info.
				"""#
		}
	}
}
