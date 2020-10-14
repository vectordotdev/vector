package metadata

components: sinks: nats: {
	title:             "NATS"
	short_description: "Streams log events to a [NATS][urls.nats] on a NATS subject."
	long_description:  "NATS.io is a simple, secure and high performance open source messaging system for cloud native applications, IoT messaging, and microservices architectures. NATS.io is a Cloud Native Computing Foundation project."

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		development:   "beta"
		egress_method: "stream"
		function:      "transmit"
		service_providers: []
	}

	features: {
		buffer: enabled:      false
		compression: enabled: false
		encoding: codec: {
			enabled: true
			default: null
			enum: ["json", "text"]
		}
		healthcheck: enabled: true
		request: enabled:     false
		tls: enabled:         false
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
		url: {
			description: "The NATS URL to connect to. The url _must_ take the form of `nats://server:port`."
			groups: ["tcp"]
			required: true
			warnings: []
			type: string: {
				examples: ["nats://demo.nats.io", "nats://127.0.0.1:4222"]
			}
		}
		subject: {
			description: "The NATS subject to publish messages to."
			required:    true
			warnings: []
			type: string: {
				default: null
				examples: ["foo", "time.us.east", "time.*.east", "time.>", ">"]
			}
		}
	}

	input: {
		logs:    true
		metrics: false
	}
}
