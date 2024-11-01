package metadata

components: sources: http_server: {
	_port: 80

	title: "HTTP Server"
	alias: "http"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["aggregator", "sidecar"]
		development:   "stable"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		acknowledgements: true
		multiline: enabled: false
		codecs: {
			enabled:         true
			default_framing: "`newline_delimited` for codecs other than `native`, which defaults to `length_delimited`"
		}
		receive: {
			from: {
				service: services.http

				interface: {
					socket: {
						direction: "incoming"
						port:      _port
						protocols: ["http"]
						ssl: "optional"
					}
				}
			}

			tls: {
				enabled:                true
				can_verify_certificate: true
				enabled_default:        false
			}
		}
		auto_generated: true
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.http_server.configuration

	output: logs: {
		text: {
			description: "An individual line from a `text/plain` request"
			fields: {
				message: {
					description:   "The raw line from the incoming payload."
					relevant_when: "encoding == \"text\""
					required:      true
					type: string: {
						examples: ["Hello world"]
					}
				}
				path: {
					description: "The HTTP path the event was received from. The key can be changed using the `path_key` configuration setting"
					required:    true
					type: string: {
						examples: ["/", "/logs/event712"]
					}
				}
				source_type: {
					description: "The name of the source type."
					required:    true
					type: string: {
						examples: ["http_server"]
					}
				}
				timestamp: fields._current_timestamp
			}
		}
		structured: {
			description: "An individual line from an `application/json` request"
			fields: {
				"*": {
					common:        false
					description:   "Any field contained in your JSON payload"
					relevant_when: "encoding != \"text\""
					required:      false
					type: "*": {}
				}
				path: {
					description: "The HTTP path the event was received from. The key can be changed using the `path_key` configuration setting"
					required:    true
					type: string: {
						examples: ["/", "/logs/event712"]
					}
				}
				timestamp: fields._current_timestamp
			}
		}
	}

	examples: [
		{
			_path:       "/"
			_line:       "Hello world"
			_user_agent: "my-service/v2.1"
			title:       "text/plain"

			configuration: {
				address:  "0.0.0.0:\(_port)"
				encoding: "text"
				headers: ["User-Agent"]
			}
			input: """
				```http
				POST \( _path ) HTTP/1.1
				Content-Type: text/plain
				User-Agent: \( _user_agent )
				X-Forwarded-For: \( _values.local_host )

				\( _line )
				```
				"""
			output: [{
				log: {
					host:          _values.local_host
					message:       _line
					timestamp:     _values.current_timestamp
					path:          _path
					"User-Agent":  _user_agent
					"source_type": "http_server"
				}
			}]
		},
		{
			_path:       "/events"
			_path_key:   "vector_http_path"
			_line:       "{\"key\": \"val\"}"
			_user_agent: "my-service/v2.1"
			title:       "application/json"

			configuration: {
				address:  "0.0.0.0:\(_port)"
				encoding: "json"
				headers: ["User-Agent"]
				_path:    _path
				path_key: _path_key
			}
			input: """
				```http
				POST \( _path ) HTTP/1.1
				Content-Type: application/json
				User-Agent: \( _user_agent )
				X-Forwarded-For: \( _values.local_host )
				\( _line )
				```
				"""
			output: [{
				log: {
					host:          _values.local_host
					key:           "val"
					timestamp:     _values.current_timestamp
					_path_key:     _path
					"User-Agent":  _user_agent
					"source_type": "http_server"
				}
			}]
		},
	]

	telemetry: metrics: {
		http_server_handler_duration_seconds: components.sources.internal_metrics.output.metrics.http_server_handler_duration_seconds
		http_server_requests_received_total:  components.sources.internal_metrics.output.metrics.http_server_requests_received_total
		http_server_responses_sent_total:     components.sources.internal_metrics.output.metrics.http_server_responses_sent_total
	}

	how_it_works: {
		decompression: {
			title: "Decompression"
			body: """
				Received body is decompressed according to `Content-Encoding` header.
				Supported algorithms are `gzip`, `deflate`, `snappy`, and `zstd`.
				"""
		}
	}
}
