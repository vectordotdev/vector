package metadata

components: sources: envoy_als: {
	_grpc_port: 9999

	title: "Envoy ALS"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "aggregator"]
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		acknowledgements: false
		multiline: enabled: false
		receive: {
			from: {
				service: services.envoy

				interface: socket: {
					direction: "incoming"
					port:      _grpc_port
					protocols: ["tcp"]
					ssl: "optional"
				}
			}
			tls: {
				// enabled per listener below
				enabled: false
			}
		}
	}

	support: {
		requirements: []
		warnings: [
			"""
				The `envoy_als` source only supports HTTP log events at this time. TCP logs will not be ingested
				and will generate a warning in Vector logs if sent. Please make sure you are using HTTP access logs in
				you Envoy configuration.
				""",
		]
		notices: [
			"""
			The envoy protos were generated from version v1.24.1. Results may vary if using a different version.
			"""
		]
	}

	installation: {
		platform_name: null
	}

	configuration: {
		grpc: {
			description: "Configuration options for the gRPC server."
			required:    true
			type: object: {
				examples: [{address: "0.0.0.0:\(_grpc_port)"}]
				options: {
					address: {
						description: """
						The gRPC address to listen for connections on. It _must_ include a port.
						"""
						required: true
						type: string: {
							examples: ["0.0.0.0:\(_grpc_port)"]
						}
					}
					tls: configuration._tls_accept & {_args: {
						can_verify_certificate: true
						enabled_default:        false
					}}
				}
			}
		}
	}

	configuration_examples: [
		{
			title: "Envoy ALS Defaults"
			configuration: {
				envoy_als: {
					grpc: {
						address: "0.0.0.0:\(_grpc_port)"
						tls: {
							enabled:  true
							crt_file: "/etc/ssl/certs/vector.pem"
							key_file: "/etc/ssl/private/vector.key"
						}
					}
				}
			}
		},
	]

	output: {
		logs: event: {
			description: "An individual log event from a batch of events received by an Envoy ALS stream"
			fields: {
				identifier: {
					description: "Added to each log event on the stream. See https://www.envoyproxy.io/docs/envoy/v1.24.1/api-v3/service/accesslog/v3/als.proto#envoy-v3-api-msg-service-accesslog-v3-streamaccesslogsmessage-identifier for more details."
					required:    false
					common:      true
					type: object: {
						examples: [
							{
								"log_name": "my-envoy-als-logs"
								"node": {
									"id": "my-node"
									"cluster": "my-cluster"
									"metadata": {...}
									"dynamic_parameters": {...}
									"locality": {...}
									"user_agent_name": "envoy"
									"user_agent_version_type": {...}
									"client_features": []
								}
							},
						]
					}
				}
				http_log: {
					description: "Fields for an http log entry. See https://www.envoyproxy.io/docs/envoy/v1.24.1/api-v3/data/accesslog/v3/accesslog.proto#data-accesslog-v3-httpaccesslogentry for more details."
					required:    false
					common:      true
					type: object: {
						examples: [
							{
								"common_properties": {...}
								"protocol_version": "HTTP11"
								"request": {...}
								"response": {...}
							}
						]
					}
				}
			}
		}
	}

	how_it_works: {
		tls: {
			title: "Envoy ALS"
			body:  """
				  In your Envoy configuration, create an `access_log` entry that points to the running Vector
					instance with the Envoy ALS source configured. Envoy will then stream logs to Vector on the
					interval configured in Envoy.
				  """
		}
	}
}
