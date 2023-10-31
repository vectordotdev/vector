package metadata

components: sources: amqp: {
	title: "AMQP"

	features: {
		auto_generated:   true
		acknowledgements: false
		collect: {
			checkpoint: enabled: false
			from: {
				service: services.amqp
				interface: {
					socket: {
						api: {
							title: "AMQP protocol"
							url:   urls.amqp_protocol
						}
						direction: "incoming"
						port:      5672
						protocols: ["tcp"]
						ssl: "optional"
					}
				}
			}
		}
		multiline: enabled: false
	}

	classes: {
		commonly_used: true
		deployment_roles: ["aggregator"]
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	support: components._amqp.support

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.amqp.configuration

	output: logs: record: {
		description: "An individual AMQP record."
		fields: {
			message: {
				description: "The raw line from the AMQP record."
				required:    true
				type: string: {
					examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
					syntax: "literal"
				}
			}
			offset: {
				description: "The AMQP offset at the time the record was retrieved."
				required:    true
				type: uint: {
					examples: [100]
					unit: null
				}
			}
			timestamp: fields._current_timestamp & {
				description: "The timestamp encoded in the AMQP message or the current time if it cannot be fetched."
			}
			exchange: {
				description: "The AMQP exchange that the record came from."
				required:    true
				type: string: {
					examples: ["topic"]
					syntax: "literal"
				}
			}
		}
	}

	how_it_works: components._amqp.how_it_works
}
