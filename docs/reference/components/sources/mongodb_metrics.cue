package metadata

components: sources: mongodb_metrics: {
	title:       "MongoDB Metrics"
	description: "[MongoDB][urls.mongodb] is a general purpose, document-based, distributed database built for modern application developers and for the cloud era."

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "sidecar"]
		development:   "beta"
		egress_method: "batch"
	}

	features: {
		collect: {
			checkpoint: enabled: false
			from: {
				name:     "MongoDB Server"
				thing:    "a \(name)"
				url:      urls.mongodb
				versions: null

				interface: {
					socket: {
						api: {
							title: "MongoDB serverStatus command"
							url:   urls.mongodb_command_server_status
						}
						direction: "outgoing"
						protocols: ["tcp"]
						ssl: "optional"
					}
				}
			}
		}
		multiline: enabled: false
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
			"User from endpoint should have enough privileges for running [serverStatus][urls.mongodb_command_server_status] command",
		]

		warnings: []
		notices: []
	}

	configuration: {
		endpoint: {
			description: "MongoDB [Connection String URI Format][urls.mongodb_connection_string_uri_format]"
			required:    true
			type: "string": {
				examples: ["mongodb://localhost:27017"]
			}
		}
		interval_secs: {
			description: "The interval between scrapes."
			common:      true
			required:    false
			type: uint: {
				default: 15
				unit:    "seconds"
			}
		}
		namespace: {
			description: "The namespace of metrics. Disabled if empty."
			common:      false
			required:    false
			type: string: default: "mongodb"
		}
	}

	output: metrics: _mongodb_metrics & {
		vector_collect_completed_total:      _vector_collect_completed_total
		vector_collect_duration_nanoseconds: _vector_collect_duration_nanoseconds
		vector_request_error_total:          _vector_request_error_total
	}

	how_it_works: {
		mod_status: {
			title: "MongoDB `serverStatus` command"
			body: """
				The [serverStatus][urls.mongodb_command_server_status] command
				returns a document that provides an overview of the database’s
				state. The output fields vary depending on the version of
				MongoDB, underlying operating system platform, the storage
				engine, and the kind of node, including `mongos`, `mongod` or
				`replica set` member.
				"""
		}
	}
}
