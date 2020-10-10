package metadata

components: sources: syslog: {
  title: "#{component.title}"
  short_description: "Ingests data through the [Syslog protocol][urls.syslog_5424] and outputs log events."
  long_description: "[Syslog][urls.syslog] stands for System Logging Protocol and is a standard protocol used to send system log or event messages to a specific server, called a syslog server. It is used to collect various device logs from different machines and send them to a central location for monitoring and review."

  classes: {
    commonly_used: true
    deployment_roles: ["service"]
	egress_method: "stream"
    function: "receive"
  }

  features: {
    checkpoint: enabled: false
    multiline: enabled: false
    tls: {
      enabled: true
      can_enable: true
      can_verify_certificate: true
      can_verify_hostname: false
      enabled_default: false
    }
  }

  statuses: {
    delivery: "best_effort"
    development: "stable"
  }

  support: {

    platforms: {
      "aarch64-unknown-linux-gnu": true
      "aarch64-unknown-linux-musl": true
      "x86_64-apple-darwin": true
      "x86_64-pc-windows-msv": true
      "x86_64-unknown-linux-gnu": true
      "x86_64-unknown-linux-musl": true
    }

    requirements: [
      """
      This component exposes a configured port. You must ensure your network allows access to this port.
      """,
    ]
    warnings: []
  }

  configuration: {
    address: {
      description: "The TCP or UDP address to listen for connections on, or \"systemd#N\" to use the Nth socket passed by systemd socket activation."
      required: true
      warnings: []
      type: string: {
        examples: ["0.0.0.0:514","systemd","systemd#2"]
      }
    }
    host_key: {
      common: false
      description: "The key name added to each event representing the current host. This can also be globally set via the [global `host_key` option][docs.reference.global-options#host_key]."
      required: false
      warnings: []
      type: string: {
        default: "host"
      }
    }
    max_length: {
      common: false
      description: "The maximum bytes size of incoming messages before they are discarded."
      required: false
      warnings: []
      type: uint: {
        default: 102400
        unit: "bytes"
      }
    }
    mode: {
      description: "The input mode."
      required: true
      warnings: []
      type: string: {
        enum: {
          tcp: "Read incoming Syslog data over the TCP protocol."
          udp: "Read incoming Syslog data over the UDP protocol."
          unix: "Read uncoming Syslog data through a Unix socker."
        }
      }
    }
    path: {
      description: "The unix socket path. *This should be absolute path.*\n"
      required: true
      warnings: []
      type: string: {
        examples: ["/path/to/socket"]
      }
    }
  }

  output: logs: line: {
		description: "An individual event from Syslog."
		fields: {
			host:      fields._local_host
			message:   fields._raw_line
			timestamp: fields._current_timestamp
		}
	}

	examples: log: [
		{
			_line: #"""
				2019-02-13T19:48:34+00:00 [info] Started GET "/" for 127.0.0.1
				"""#
			title: "Syslog line"
			configuration: {}
			input: """
				```text
				\( _line )
				```
				"""
			output: {
				timestamp: _values.current_timestamp
				message:   _line
				host:      _values.local_host
			}
		}]
}
