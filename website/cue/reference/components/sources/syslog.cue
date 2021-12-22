package metadata

components: sources: syslog: {
	_port: 514

	title: "Syslog"

	classes: sources.socket.classes

	features: {
		multiline: sources.socket.features.multiline

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

	configuration: {
		address: {
			description:   "The address to listen for connections on, or `systemd#N` to use the Nth socket passed by systemd socket activation. If an address is used it _must_ include a port."
			relevant_when: "mode = `tcp` or `udp`"
			required:      true
			type: string: {
				examples: ["0.0.0.0:\(_port)", "systemd", "systemd#3"]
			}
		}
		host_key: {
			category:    "Context"
			common:      false
			description: """
				The key name added to each event representing the current host. This can also be globally set via the
				[global `host_key` option](\(urls.vector_configuration)/global-options#log_schema.host_key).
				"""
			required:    false
			type: string: {
				default: "host"
			}
		}
		max_length: {
			common:      true
			description: "The maximum buffer size of incoming messages. Messages larger than this are truncated."
			required:    false
			type: uint: {
				default: 102400
				unit:    "bytes"
			}
		}
		mode: {
			description: "The type of socket to use."
			required:    true
			type: string: {
				enum: {
					tcp:  "TCP socket."
					udp:  "UDP socket."
					unix: "Unix domain stream socket."
				}
			}
		}
		path: {
			description:   "The unix socket path. *This should be an absolute path*."
			relevant_when: "mode = `unix`"
			required:      true
			type: string: {
				examples: ["/path/to/socket"]
			}
		}
		connection_limit: {
			common:        false
			description:   "The max number of TCP connections that will be processed."
			relevant_when: "mode = `tcp`"
			required:      false
			type: uint: {
				default: null
				unit:    "concurrency"
			}
		}
	}

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
			host: fields._local_host
			hostname: {
				description: "The hostname extracted from the Syslog line. (`host` is also this value if it exists in the log.)"
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
				description: "The upstream hostname. In the case where `mode` = `\"unix\"` the socket path will be used. (`host` is also this value if `hostname` does not exist in the log.)"
				required:    true
				type: string: {
					examples: ["127.0.0.1"]
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
			"*": {
				description: "In addition to the defined fields, any Syslog 5424 structured fields are parsed and inserted as root level fields."
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
				hostname:    _hostname
				appname:     _app_name
				procid:      _procid
				msgid:       _msgid
				iut:         _iut
				eventSource: _event_source
				eventID:     _event_id
				message:     _message
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

				If parsing fails, Vector includes the entire Syslog line in the `message`
				key. If you find this happening often, we recommend using the
				[`socket` source](\(urls.vector_socket_source)) combined with
				[regex parsing](\(urls.vrl_functions)/#parse_regex) to implement your own custom
				ingestion and parsing scheme. Alternatively, you can [open an
				issue](\(urls.new_feature_request)) to request support for your specific format.
				"""
		}
	}

	telemetry: metrics: {
		events_in_total:                 components.sources.internal_metrics.output.metrics.events_in_total
		connection_read_errors_total:    components.sources.internal_metrics.output.metrics.connection_read_errors_total
		processed_bytes_total:           components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:          components.sources.internal_metrics.output.metrics.processed_events_total
		component_received_bytes_total:  components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_events_total: components.sources.internal_metrics.output.metrics.component_received_events_total
		utf8_convert_errors_total:       components.sources.internal_metrics.output.metrics.utf8_convert_errors_total
	}
}
