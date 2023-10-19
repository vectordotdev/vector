package metadata

components: sources: journald: {
	title: "Journald"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		deployment_roles: ["daemon"]
		development:   "stable"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		collect: {
			checkpoint: enabled: true
			from: {
				service: services.journald

				interface: binary: {
					name: "journalctl"
					permissions: unix: group: "systemd-journal"
				}
			}
		}
		multiline: enabled: false
	}

	support: {
		targets: {
			"x86_64-apple-darwin":   false
			"x86_64-pc-windows-msv": false
		}

		requirements: [
			"""
				This source requires permissions to run `journalctl`. When installed from a package manager this should be
				handled automatically, otherwise ensure the running user is part of the `systemd-journal` group.
				""",
		]
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.journald.configuration

	output: logs: {
		event: {
			description: "A Journald event"
			fields: {
				host: fields._local_host
				message: {
					description: "The raw line from the file."
					required:    true
					type: string: {
						examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
					}
				}
				source_type: {
					description: "The name of the source type."
					required:    true
					type: string: {
						examples: ["journald"]
					}
				}
				timestamp: fields._current_timestamp & {
					description: "The time at which the event appeared in the journal."
				}
				"*": {
					common:      false
					description: "Any Journald field"
					required:    false
					type: string: {
						default: null
						examples: ["/usr/sbin/ntpd", "c36e9ea52800a19d214cb71b53263a28"]
					}
				}
			}
		}
	}

	examples: [
		{
			title: "Sample Output"

			configuration: {}
			input: """
				2019-07-26 20:30:27 reply from 192.168.1.2: offset -0.001791 delay 0.000176, next query 1500s
				"""
			output: [{
				log: {
					timestamp:                _values.current_timestamp
					source_type:              "journald"
					message:                  "reply from 192.168.1.2: offset -0.001791 delay 0.000176, next query 1500s"
					host:                     _values.local_host
					"__REALTIME_TIMESTAMP":   "1564173027000443"
					"__MONOTONIC_TIMESTAMP":  "98694000446"
					"_BOOT_ID":               "124c781146e841ae8d9b4590df8b9231"
					"SYSLOG_FACILITY":        "3"
					"_UID":                   "0"
					"_GID":                   "0"
					"_CAP_EFFECTIVE":         "3fffffffff"
					"_MACHINE_ID":            "c36e9ea52800a19d214cb71b53263a28"
					"PRIORITY":               "6"
					"_TRANSPORT":             "stdout"
					"_STREAM_ID":             "92c79f4b45c4457490ebdefece29995e"
					"SYSLOG_IDENTIFIER":      "ntpd"
					"_PID":                   "2156"
					"_COMM":                  "ntpd"
					"_EXE":                   "/usr/sbin/ntpd"
					"_CMDLINE":               "ntpd: [priv]"
					"_SYSTEMD_CGROUP":        "/system.slice/ntpd.service"
					"_SYSTEMD_UNIT":          "ntpd.service"
					"_SYSTEMD_SLICE":         "system.slice"
					"_SYSTEMD_INVOCATION_ID": "496ad5cd046d48e29f37f559a6d176f8"
				}
			}]
		},
	]

	how_it_works: {
		communication_strategy: {
			title: "Communication Strategy"
			body:  """
				To ensure the `journald` source works across all platforms, Vector interacts
				with the systemd journal via the `journalctl` command. This is accomplished by
				spawning a [subprocess](\(urls.rust_subprocess)) that Vector interacts
				with. If the `journalctl` command is not in the environment path you can
				specify the exact location via the `journalctl_path` option. For more
				information on this communication strategy please see
				[issue #1473](\(urls.vector_issues)/1473).
				"""
		}
		non_ascii: {
			title: "Non-ASCII Messages"
			body: """
				When `journald` has stored a message that is not strict ASCII,
				`journalctl` will output it in an alternate format to prevent data
				loss. Vector handles this alternate format by translating such messages
				into UTF-8 in "lossy" mode, where characters that are not valid UTF-8
				are replaced with the Unicode replacement character, `�`.
				"""
		}
	}
}
