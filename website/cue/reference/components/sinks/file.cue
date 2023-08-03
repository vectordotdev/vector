package metadata

components: sinks: file: {
	title: "File"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"

		development:   "stable"
		egress_method: "stream"
		service_providers: []
		stateful: false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			compression: {
				enabled: true
				default: "none"
				algorithms: ["none", "gzip", "zstd"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					framing: true
					enum: ["json", "text"]
				}
			}
			request: enabled: false
			tls: enabled:     false
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: base.components.sinks.file.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		dir_and_file_creation: {
			title: "File & Directory Creation"
			body: """
				Vector will attempt to create the entire directory structure
				and the file when emitting events to the file sink. This
				requires that the Vector agent have the correct permissions
				to create and write to files in the specified directories.
				"""
		}

		durability: {
			title: "Durability of Created Files"
			body: """
				Vector makes no attempt to ensure the files output by
				this sink are durably written to disk by using any of
				the "sync" write modes. As such, this sink only
				ensures that the operating system does not generate an
				error, it does not wait until the data is written to
				disk before acknowledging the events.
				"""
		}
	}
}
