package metadata

components: sinks: journald: {
	title: "Journald"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		stateful:      false
		egress_method: "stream"
		service_providers: []
	}

	features: {
		acknowledgements: true
		healthcheck: {enabled: false}
		send: {
			batch: {
				enabled: false
				common:  false
			}
			compression: {
				enabled: false
				default: null
				algorithms: []
				levels: []
			}
			encoding: {
				enabled: false
			}
			request: {
				enabled: false
			}
			tls: {
				enabled: false
			}
		}
	}

	support: {
		requirements: [
			"""
				The systemd journal socket must be accessible to Vector.
				""",
		]
		warnings: []
		notices: []
	}

	configuration: {
		journald_path: {
			common:      false
			description: "Path to the journald socket."
			required:    false
			type: string: {
				default: "/run/systemd/journal/socket"
				examples: ["/run/systemd/journal/socket"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	how_it_works: {
		state: {
			title: "State"
			body: """
				This component is stateless, meaning its behavior is consistent across each input.
				"""
		}

		context: {
			title: "Context"
			body: """
				The journald sink sends all fields of a log event to the systemd journal.
				Field names will be sanitized according to the journald protocol, which requires
				uppercase alphanumeric characters, with certain characters replaced by underscores.
				"""
		}

		field_mapping: {
			title: "Field Mapping"
			body: """
				All fields from the log event are sent to journald. Field names are sanitized
				according to journald protocol requirements:

				- Fields are converted to uppercase
				- '=', '\n', and '.' characters are replaced with '_'
				- All other non ascii alphanumeric characters are skipped
				- Field names are truncated to 64 characters
				"""
		}

		large_payloads: {
			title: "Large Payloads"
			body: """
				For large payloads that exceed the Unix datagram size limit, the sink
				automatically uses the memfd mechanism described in the journald native
				protocol specification.
				"""
		}
	}
}
