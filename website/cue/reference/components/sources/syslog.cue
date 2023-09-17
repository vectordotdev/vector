package metadata

components: sources: syslog: {
	_port: 514

	title: "Syslog"

	classes: sources.socket.classes

	features: {
		auto_generated:   true
		acknowledgements: sources.socket.features.acknowledgements
		multiline:        sources.socket.features.multiline
		receive: {
			from: {
				service: services.syslog

				interface: socket: {
					api: {
						title: "Syslog"
						url:   urls.syslog
					}
					direction: "incoming"
					port:      _port
					protocols: ["tcp", "unix", "udp"]
					ssl: "optional"
				}
			}
			receive_buffer_bytes: {
				enabled:       true
				relevant_when: "mode = `tcp` or mode = `udp`"
			}
			keepalive: enabled: true
			tls: sources.socket.features.receive.tls
		}
	}

	support: {
		targets: sources.socket.support.targets

		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.syslog.configuration

	output: logs: line: {
		description: "An individual Syslog event"
		fields: {
			appname: {
				description: "The appname extracted from the Syslog formatted line. If a appname is not found, then the key will not be added."
				required:    true
				type: string: {
					examples: ["app-name"]
				}
			}
			host: {
				description: "Same as `hostname` if that field is set, or the IP address of the peer otherwise."
				required:    true
				type: string: {
					examples: ["my.host.com", "127.0.0.1"]
				}
			}
			hostname: {
				description: "The `hostname` field extracted from the Syslog line. If a `hostname` field is found, `host` is also set to this value."
				required:    true
				type: string: {
					examples: ["my.host.com"]
				}
			}
			facility: {
				description: "The facility extracted from the Syslog line. If a facility is not found, then the key will not be added."
				required:    true
				type: string: {
					examples: ["1"]
				}
			}
			message: {
				description: "The message extracted from the Syslog line."
				required:    true
				type: string: {
					examples: ["Hello world"]
				}
			}
			msgid: {
				description: "The msgid extracted from the Syslog line. If a msgid is not found, then the key will not be added."
				required:    true
				type: string: {
					examples: ["ID47"]
				}
			}
			procid: {
				description: "The procid extracted from the Syslog line. If a procid is not found, then the key will not be added."
				required:    true
				type: string: {
					examples: ["8710"]
				}
			}
			severity: {
				description: "The severity extracted from the Syslog line. If a severity is not found, then the key will not be added."
				required:    true
				type: string: {
					examples: ["notice"]
				}
			}
			source_ip: {
				description: "The IP address of the client. In the case where `mode` = `\"unix\"` the socket path will be used."
				required:    true
				type: string: {
					examples: ["127.0.0.1"]
				}
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["syslog"]
				}
			}
			timestamp: {
				description: "The time extracted from the Syslog formatted line. If parsing fails, then the exact time the event was ingested into Vector is used."
				required:    true
				type: timestamp: {}
			}
			version: {
				description: "The version extracted from the Syslog line. If a version is not found, then the key will not be added."
				required:    true
				type: uint: {
					examples: [1]
					unit: null
				}
			}
			client_metadata: fields._client_metadata
			"*": {
				description: "In addition to the defined fields, any [Syslog 5424 structured fields](https://datatracker.ietf.org/doc/html/rfc5424#section-6.3) are parsed and inserted, namespaced under the name of each structured data section."
				required:    true
				type: string: {
					examples: ["hello world"]
				}
			}
		}
	}

	examples: [
		{
			_app_name:     "non"
			_event_id:     "1011"
			_event_source: "Application"
			_hostname:     "dynamicwireless.name"
			_iut:          "3"
			_message:      "Try to override the THX port, maybe it will reboot the neural interface!"
			_msgid:        "ID931"
			_procid:       "2426"
			_timestamp:    "2020-03-13T20:45:38.119Z"
			title:         "Syslog Event"
			configuration: {}
			input: """
				<13>1 \(_timestamp) \(_hostname) \(_app_name) \(_procid) \(_msgid) [exampleSDID@32473 iut="\(_iut)" eventSource="\(_event_source)" eventID="\(_event_id)"] \(_message)
				"""
			output: log: {
				severity:    "notice"
				facility:    "user"
				timestamp:   _timestamp
				host:        _values.local_host
				source_ip:   _values.remote_host
				source_type: "syslog"
				hostname:    _hostname
				appname:     _app_name
				procid:      _procid
				msgid:       _msgid
				"exampleSDID@32473": {
					iut:         _iut
					eventSource: _event_source
					eventID:     _event_id
				}
				message: _message
			}
		},
	]

	how_it_works: {
		line_delimiters: {
			title: "Line Delimiters"
			body: """
				Each line is read until a new line delimiter, the `0xA` byte, is found.
				"""
		}

		parsing: {
			title: "Parsing"
			body:  """
				Vector makes a _best effort_ to parse the various Syslog formats out in the wild.
				This includes [RFC 6587](\(urls.syslog_6587)), [RFC 5424](\(urls.syslog_5424)),
				[RFC 3164](\(urls.syslog_3164)), and other common variations (such as the Nginx
				Syslog style). It's unfortunate that the Syslog specification isn't more
				accurately followed, but we hope that Vector insulates you from these deviations.

				If parsing fails, Vector will raise an error. If you find this happening often,
				we recommend using the [`socket` source](\(urls.vector_socket_source)) combined with
				[regex parsing](\(urls.vrl_functions)/#parse_regex) to implement your own custom
				ingestion and parsing scheme, or [syslog parsing](\(urls.vrl_functions)/#parse_syslog) and
				manually handle any errors. Alternatively, you can [open an
				issue](\(urls.new_feature_request)) to request support for your specific format.
				"""
		}
	}

	telemetry: metrics: {
		connection_read_errors_total: components.sources.internal_metrics.output.metrics.connection_read_errors_total
		utf8_convert_errors_total:    components.sources.internal_metrics.output.metrics.utf8_convert_errors_total
	}
}
