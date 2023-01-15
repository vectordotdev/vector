package metadata

components: sources: http_server: {
	_port: 80

	title: "HTTP Server"
	alias: "http"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["aggregator", "sidecar"]
		development:   "beta"
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
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		acknowledgements: configuration._source_acknowledgements
		address: {
			description: "The address to accept connections on. The address _must_ include a port."
			required:    true
			type: string: {
				examples: ["0.0.0.0:\(_port)", "localhost:\(_port)"]
			}
		}
		encoding: {
			common:      true
			description: "The expected encoding of received data. Note that for `json` and `ndjson` encodings, the fields of the JSON objects are output as separate fields."
			required:    false
			type: string: {
				default: "text"
				enum: {
					text:   "Newline-delimited text, with each line forming a message."
					ndjson: "Newline-delimited JSON objects, where each line must contain a JSON object."
					json:   "Array of JSON objects, which must be a JSON array containing JSON objects."
					binary: "Binary or text, whole http request body is considered as one message."
				}
			}
		}
		headers: {
			common:      false
			description: "A list of HTTP headers to include in the log event. These will override any values included in the JSON payload with conflicting names."
			required:    false
			type: array: {
				default: null
				items: type: string: {
					examples: ["User-Agent", "X-My-Custom-Header"]
				}
			}
		}
		auth: configuration._http_basic_auth
		query_parameters: {
			common:      false
			description: "A list of URL query parameters to include in the log event. These will override any values included in the body with conflicting names."
			required:    false
			type: array: {
				default: null
				items: type: string: {
					examples: ["application", "source"]
				}
			}
		}
		path: {
			common:      false
			description: "The URL path on which log event POST requests shall be sent."
			required:    false
			type: string: {
				default: "/"
				examples: ["/event/path", "/logs"]
			}
		}
		strict_path: {
			common: false
			description: """
				If set to `true`, only requests using the exact URL path specified in `path` will be accepted;
				otherwise requests sent to a URL path that starts with the value of `path` will be accepted.
				With `strict_path` set to `false` and `path` set to `""`, the configured HTTP source will
				accept requests from any URL path.
				"""
			required: false
			type: bool: default: true
		}
		path_key: {
			common:      false
			description: "The event key in which the requested URL path used to send the request will be stored."
			required:    false
			type: string: {
				default: "path"
				examples: ["vector_http_path"]
			}
		}
		method: {
			common:      false
			description: "Specifies the action of the HTTP request."
			required:    false
			type: string: {
				default: "POST"
				enum: {
					"HEAD":   "HTTP HEAD method."
					"GET":    "HTTP GET method."
					"PUT":    "HTTP PUT method."
					"POST":   "HTTP POST method."
					"PATCH":  "HTTP PATCH method."
					"DELETE": "HTTP DELETE method."
				}
			}
		}
	}
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
		component_errors_total:               components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		events_in_total:                      components.sources.internal_metrics.output.metrics.events_in_total
		http_bad_requests_total:              components.sources.internal_metrics.output.metrics.http_bad_requests_total
		parse_errors_total:                   components.sources.internal_metrics.output.metrics.parse_errors_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
	}

	how_it_works: {
		decompression: {
			title: "Decompression"
			body: """
				Received body is decompressed according to `Content-Encoding` header.
				Supported algorithms are `gzip`, `deflate`, and `snappy`.
				"""
		}
	}
}
