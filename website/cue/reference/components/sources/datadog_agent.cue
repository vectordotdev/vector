package metadata

components: sources: datadog_agent: {
	_port: 8080

	title: "Datadog Agent"

	description: """
		Receives observability data from a Datadog Agent over HTTP or HTTPS. For now, this is limited to logs, but will
		be expanded in the future to cover metrics and traces.
		"""

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["aggregator", "sidecar"]
		development:   "beta"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		multiline: enabled: false
		codecs: enabled:    true
		receive: {
			from: {
				service: services.datadog_agent

				interface: socket: {
					direction: "incoming"
					port:      _port
					protocols: ["http"]
					ssl: "optional"
				}
			}

			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				enabled_default:        false
			}
		}
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		acknowledgements: configuration._acknowledgements
		address:          sources.http.configuration.address
		store_api_key: {
			common:      false
			description: "When incoming events contain a Datadog API key, if this setting is set to `true` the key will kept in the event metadata and will be used if the event is sent to a Datadog sink."
			required:    false
			type: bool: default: true
		}
	}

	output: logs: line: {
		description: "An individual event from a batch of events received through an HTTP POST request sent by a Datadog Agent."
		fields: {
			message: {
				description: "The message field, containing the plain text message."
				required:    true
				type: string: {
					examples: ["Hi from erlang"]
					syntax: "literal"
				}
			}
			status: {
				description: "The status field extracted from the event."
				required:    true
				type: string: {
					examples: ["info"]
					syntax: "literal"
				}
			}
			timestamp: fields._current_timestamp
			hostname:  fields._local_host
			service: {
				description: "The service field extracted from the event."
				required:    true
				type: string: {
					examples: ["backend"]
					syntax: "literal"
				}
			}
			ddsource: {
				description: "The source field extracted from the event."
				required:    true
				type: string: {
					examples: ["java"]
					syntax: "literal"
				}
			}
			ddtags: {
				description: "The coma separated tags list extracted from the event."
				required:    true
				type: string: {
					examples: ["env:prod,region:ap-east-1"]
					syntax: "literal"
				}
			}
		}
	}

	how_it_works: {
		decompression: {
			title: "Configuring the Datadog Agent"
			body:  """
				To send logs from a Datadog Agent to this source, the [Datadog Agent](\(urls.datadog_agent_doc)) configuration
				must be updated to use:

				```yaml
				logs_config:
					dd_url: "<VECTOR_HOST>:<SOURCE_PORT>"
					use_v2_api: false # source does not yet support new v2 API
					use_http: true # this source only supports HTTP/HTTPS
					logs_no_ssl: true|false # should match source SSL configuration.
				```
				"""
		}
	}
}
