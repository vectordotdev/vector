package metadata

components: sources: nats: {
	title: "NATs"

	configuration: {
		url: {
			description: "The NATS URL to connect to. The url _must_ take the form of `nats://server:port`."
			required:    true
			warnings: []
			type: string: {
				examples: ["nats://demo.nats.io", "nats://127.0.0.1:4222"]
				syntax: "literal"
			}
		}
		subject: {
			description: "The NATS subject to publish messages to."
			required:    true
			warnings: []
			type: string: {
				examples: ["{{ host }}", "foo", "time.us.east", "time.*.east", "time.>", ">"]
				syntax: "template"
			}
		}
		name: {
			common:      false
			description: "A name assigned to the NATS connection."
			required:    false
			type: string: {
				default: "vector"
				examples: ["foo", "API Name Option Example"]
				syntax: "literal"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}
}
