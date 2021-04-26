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
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			compression: enabled: false
			encoding: enabled:    false
			send_buffer_bytes: {
				enabled:       true
				relevant_when: "mode = `tcp` or mode = `udp`"
			}
			keepalive: enabled: true
			request: enabled:   false
			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
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
						ssl: "required"
					}
				}
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

	configuration: sinks.socket.configuration & {
		"type": "type": string: enum: syslog: "The type of this component."
		format: {
			common:      true
			description: "The Syslog format used to send message, RFC3164 and RFC5424 are supported."
			required:    false
			warnings: []
			type: string: {
				default: "rfc5424"
				enum: {
					rfc3164: "Format message according to [RFC3164](https://tools.ietf.org/html/rfc3164."
					rfc5424: "Format message according to [RFC5424](https://tools.ietf.org/html/rfc5424."
				}
				syntax: "literal"
			}
		}
		include_extra_fields: {
			common:      false
			description: "If this is set to `true` all fields that have not been mapped to a Syslog field will be included as structured data in the resulting message."
			required:    false
			warnings: []
			type: bool: default: false
		}
		appname_key: {
			common:      false
			description: "The key for the field that will be used as the `APP-NAME`."
			required:    false
			warnings: []
			type: string: {
				default: "appname"
				examples: ["application_name"]
				syntax: "literal"
			}
		}
		facility_key: {
			common: false
			description: """
				The key for the field that will be used as the Syslog facility. The following value will be recognized as
				valid facility: integer from 0 (included) to 23 (included) and the matching string representation: `kern`, `user`,
				`mail`, `daemon`, `auth`, `syslog`, `lpr`, `news`, `uucp`, `cron`, `authpriv`, `ftp`, `ntp`, `audit`, `alert`, `clockd`,
				`local0`, `local1`, `local2`, `local3`, `local4`, `local5`, `local6` and `local7`. Please check the
				[RFC5424](https://tools.ietf.org/html/rfc5424) for additional information.
				"""
			required: false
			warnings: []
			type: string: {
				default: "facility"
				examples: ["facility"]
				syntax: "literal"
			}
		}
		host_key: {
			common:      false
			description: "The key for the field that will be used to as the host in Syslog messages. This overrides the [global `host_key` option][docs.reference.configuration.global-options#host_key]."
			required:    false
			warnings: []
			type: string: {
				default: "hostname"
				examples: ["host"]
				syntax: "literal"
			}
		}
		msgid_key: {
			common:      false
			description: "The key for the field that will be used as the Syslog `MSGID`."
			required:    false
			warnings: []
			type: string: {
				default: "msgid"
				examples: ["msg_is"]
				syntax: "literal"
			}
		}
		procid_key: {
			common:      false
			description: "The key for the field that will be used as the Syslog `PROCID`."
			required:    false
			warnings: []
			type: string: {
				default: "procid"
				examples: ["proc_id"]
				syntax: "literal"
			}
		}
		severity_key: {
			common: false
			description: """
				The key for the field that will be used as the Syslog severity. The following value will be recognized as
				valid severity: integer from 0 (included) to 7 (included) and the matching string representation: `emerg`,
				`alert`, `crit`, `err`, `warning`, `notice`, `info` and `debug`. Please check the
				[RFC5424](https://tools.ietf.org/html/rfc5424) for additional information.
				"""
			required: false
			warnings: []
			type: string: {
				default: "severity"
				examples: ["sev"]
				syntax: "literal"
			}
		}
		default_facility: {
			common: false
			description: """
				The default facility when there is no facility in the message or when it cannot be matched to a known
				facility. Supported value are: `LOG_KERN`, `LOG_USER`, `LOG_MAIL`, `LOG_DAEMON`, `LOG_AUTH`, `LOG_SYSLOG`,
				`LOG_LPR`, `LOG_NEWS`, `LOG_UUCP`, `LOG_CRON`, `LOG_AUTHPRIV`, `LOG_FTP`, `LOG_NTP`, `LOG_AUDIT`, `LOG_ALERT`,
				`LOG_CLOCKD`, `LOG_LOCAL0`, `LOG_LOCAL1`, `LOG_LOCAL2`, `LOG_LOCAL3`, `LOG_LOCAL4`, `LOG_LOCAL5`, `LOG_LOCAL6`
				and `LOG_LOCAL7` *(TODO reword as enum and probably harmonise with the value accepted in log even)*
				"""
			required: false
			warnings: []
			type: string: {
				default: "LOG_SYSLOG"
				examples: ["LOG_UUCP"]
				syntax: "literal"
			}
		}
		default_severity: {
			common: false
			description: """
				The default severity when there is no severity in the message or when it cannot be matched to a known
				severity. Supported value are: `SEV_EMERG`, `SEV_ALERT`, `SEV_CRIT`, `SEV_ERR`, `SEV_WARNING`, `SEV_NOTICE`,
				`SEV_INFO` and `SEV_DEBUG` *(TODO: reword as enum and probably harmonise with the value accepted in log even)*
				"""
			required: false
			warnings: []
			type: string: {
				default: "SEV_DEBUG"
				examples: ["SEV_ALERT"]
				syntax: "literal"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

}
