package metadata

components: sinks: syslog: {
	title: "Syslog"

	classes: {
		commonly_used: true
		delivery:      "best_effort"
		development:   "beta"
		egress_method: "stream"
		service_providers: []
		stateful: false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			compression: enabled: false
			encoding: {
				enabled: true
				codec: {
					enabled: false
				}
			}
			send_buffer_bytes: {
				enabled:       true
				relevant_when: "mode = `tcp` or mode = `udp`"
			}
			keepalive: enabled: true
			request: enabled:   false
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      false
			}
			to: {
				service: services.syslog

				interface: {
					socket: {
						api: {
							title: "Syslog"
							url:   urls.syslog
						}
						direction: "outgoing"
						protocols: ["tcp", "udp", "unix"]
						ssl: "optional"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: [
			"""
				For RFC 5425 syslog over TLS compliance, configure `mode = "tcp"`,
				`tls.enabled = true`, `syslog.rfc = "rfc5424"`, and
				`framing.method = "octet_counting"`. The default
				`newline_delimited` framing is not RFC 5425 compliant.
				""",
			"""
				`newline_delimited` stream framing can split messages that contain
				embedded newlines, such as stack traces or multiline JSON. Use
				`framing.method = "octet_counting"` for multiline syslog messages.
				""",
			"""
				Unix datagram sockets, including typical Linux `/dev/log` sockets,
				are not supported by this sink. Use the `socket` sink with
				`mode = "unix_datagram"` for local syslog daemons that require
				datagram sockets.
				""",
			"""
				RFC 3164 timestamps are emitted in UTC.
				""",
			"""
				This sink does not expose `except_fields`, `only_fields`, or
				`timestamp_format` transformation options. Use the `socket` sink
				with `encoding.codec = "syslog"` if you need field filtering or
				timestamp formatting before serialization.
				""",
		]
	}

	configuration: generated.components.sinks.syslog.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	configuration_examples: [
		{
			title: "UDP RFC 5424"
			configuration: {
				type:    "syslog"
				mode:    "udp"
				address: "syslog.example.com:514"
				syslog: rfc: "rfc5424"
			}
		},
		{
			title: "TCP RFC 5424 with octet-counting"
			configuration: {
				type:    "syslog"
				mode:    "tcp"
				address: "syslog.example.com:514"
				framing: method: "octet_counting"
				syslog: rfc:     "rfc5424"
			}
		},
		{
			title: "RFC 5425 over TLS"
			configuration: {
				type:    "syslog"
				mode:    "tcp"
				address: "syslog.example.com:6514"
				framing: method: "octet_counting"
				syslog: rfc:     "rfc5424"
				tls: enabled:    true
			}
		},
	]
}
