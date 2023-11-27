package metadata

components: sources: fluent: {
	_port: 24224

	title: "Fluent"

	classes: {
		commonly_used: true
		delivery:      "best_effort"
		deployment_roles: ["sidecar", "aggregator"]
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		receive: {
			from: {
				service: services.fluent

				interface: socket: {
					api: {
						title: "Fluent"
						url:   urls.fluent
					}
					direction: "incoming"
					port:      _port
					protocols: ["tcp"]
					ssl: "optional"
				}
			}
			receive_buffer_bytes: {
				enabled: true
			}
			keepalive: enabled: true
			tls: sources.socket.features.receive.tls
		}
		multiline: enabled: false
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.fluent.configuration

	output: logs: line: {
		description: "A Fluent message"
		fields: {
			host: {
				description: "The IP address the fluent message was sent from."
				required:    true
				type: string: {
					examples: ["127.0.0.1"]
				}
			}
			timestamp: {
				description: "The timestamp extracted from the fluent message."
				required:    true
				type: timestamp: {}
			}
			tag: {
				description: "The tag from the fluent message."
				required:    true
				type: string: {
					examples: ["dummy.0"]
				}
			}
			client_metadata: fields._client_metadata
			"*": {
				description: "In addition to the defined fields, all fields from the fluent message are inserted as root level fields."
				required:    true
				type: string: {
					examples: ["hello world"]
				}
			}
		}
	}

	examples: [
		{
			title: "Dummy message from fluentd"
			configuration: {}
			input: """
				2021-05-20 16:23:03.021497000 -0400 dummy: {"message":"dummy"}
				"""
			output: log: {
				host:      _values.remote_host
				timestamp: "2021-05-20T20:23:03.021497Z"
				tag:       "dummy"
				message:   "dummy"
			}
		},
		{
			title: "Dummy message from fluent-bit"
			configuration: {}
			input: """
				dummy.0: [1621541848.161827000, {"message"=>"dummy"}]
				"""
			output: log: {
				host:      _values.remote_host
				timestamp: "2020-05-20T20:17:28.161827Z"
				tag:       "dummy.0"
				message:   "dummy"
			}
		},
	]

	how_it_works: {
		aggregator: {
			title: "Sending data from fluent agents to Vector aggregators"
			body: """
				If you are already running fluent agents (Fluentd or Fluent Bit) in your infrastructure, this source can
				make it easy to start getting that data into Vector.
				"""
		}

		fluentd_configuration: {
			title: "Fluentd configuration"
			body: """
				To configure Fluentd to forward to a Vector instance, you can use the following output configuration:

				```text
					<match *>
					  @type forward
					  <server>
						# update these to point to your vector instance
						name  local
						host  127.0.0.1
						port 24224
					  </server>
					  compress gzip
					</match>
				```
				"""
		}

		fluentbit_configuration: {
			title: "Fluent Bit configuration"
			body: """
				To configure Fluent Bit to forward to a Vector instance, you can use the following output configuration:

				```text
					[OUTPUT]
						Name          forward
						Match         *
						# update these to point to your vector instance
						Host          127.0.0.1
						Port          24224
				```
				"""
		}

		secure_mode: {
			title: "Secure forward mode support"
			body:  """
				The `fluent` source currently supports using TLS, but does not support the authentication part of the
				Fluent protocol including:

				- Shared key
				- Username and password

				And so these options of the secure forward output plugins for Fluent and Fluent Bit cannot be used.

				If you would find this useful, [please let us know](\(urls.vector_repo)/issues/7532).
				"""
		}
	}
}
