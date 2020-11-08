package metadata

components: sources: syslog: {
	_port: 514

	title:       "Syslog"
	description: "[Syslog](\(urls.syslog)) stands for System Logging Protocol and is a standard protocol used to send system log or event messages to a specific server, called a syslog server. It is used to collect various device logs from different machines and send them to a central location for monitoring and review."

	classes: sources.socket.classes
	features: {
		multiline: sources.socket.features.multiline

		receive: {
			from: {
				name:     "Syslog"
				thing:    "a \(name) client"
				url:      urls.syslog
				versions: null

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

			tls: sources.socket.features.receive.tls
		}
	}

	support: {
		platforms: sources.socket.support.platforms

		requirements: []
		warnings: []
		notices: []
	}

	configuration: sources.socket.configuration

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
			timestamp: fields._current_timestamp
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
				type: string: examples: ["hello world"]
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
				```text
				<13>1 \(_timestamp) \(_hostname) \(_app_name) \(_procid) \(_msgid) [exampleSDID@32473 iut="\(_iut)" eventSource="\(_event_source)" eventID="\(_event_id)"] \(_message)
				```
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
				Vector makes a _best effort_ to parse the various Syslog formats out in the
				wild. This includes [RFC 6587][urls.syslog_6587], [RFC 5424][urls.syslog_5424],
				[RFC 3164][urls.syslog_3164], and other common variations (such as the Nginx
				Syslog style). It's unfortunate that the Syslog specification is not more
				accurately followed, but we hope Vector insulates you from these deviations.

				If parsing fails, Vector will include the entire Syslog line in the `message`
				key. If you find this happening often, we recommend using the
				[`socket` source][docs.sources.socket] combined with the
				[`regex_parser` transform][docs.transforms.regex_parser] to implement your own
				ingestion and parsing scheme. Or, [open an issue](\(urls.new_feature_request))
				requesting support for your specific format.
				"""
		}
	}

	telemetry: metrics: {
		vector_connection_read_errors_total: {
			description: "The total number of errors reading datagram."
			type:        "counter"
			tags:        _component_tags & {
				mode: {
					description: ""
					required:    true
					options: {
						udp: "User Datagram Protocol"
					}
				}
			}
		}
		vector_utf8_convert_errors_total: {
			description: "The total number of errors converting bytes to a UTF-8 string in UDP mode."
			type:        "counter"
			tags:        _component_tags & {
				mode: {
					description: "The connection mode used by the component."
					required:    true
					options: {
						udp: "User Datagram Protocol"
					}
				}
			}
		}
	}
}
