package metadata

components: sources: syslog: {
	title:             "Syslog"
	short_description: "Ingests data through the [Syslog protocol][urls.syslog_5424] and outputs log events."
	long_description:  "[Syslog][urls.syslog] stands for System Logging Protocol and is a standard protocol used to send system log or event messages to a specific server, called a syslog server. It is used to collect various device logs from different machines and send them to a central location for monitoring and review."

	classes:       sources.socket.classes
	features:      sources.socket.features
	statuses:      sources.socket.statuses
	support:       sources.socket.support
	configuration: sources.socket.configuration

	output: logs: line: {
		description: "Fix me"
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
				description: "The hostname extracted from the Syslog line. (`host` is also this value if it exists in the log.)\n"
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
				description: "The upstream hostname. In the case where `mode` = `\"unix\"` the socket path will be used. (`host` is also this value if `hostname` does not exist in the log.)\n"
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
				description: "In addition to the defined fields, any Syslog 5424 structured fields are parsed and inserted as root level fields.\n"
				required:    true
				type: "*": {}
			}
		}
	}

	examples: log: [
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
			output: {
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
}
