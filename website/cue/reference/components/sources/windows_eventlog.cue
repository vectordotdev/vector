package metadata

components: sources: windows_eventlog: {
	title: "Windows Event Log"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon"]
		development:   "beta"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		acknowledgements: false
		auto_generated:   true
		collect: {
			checkpoint: enabled: false
			from: {
				service: services.windows_eventlog
				interface: binary: {
					name: "Windows Event Log API"
				}
			}
		}
		multiline: enabled: false
	}

	support: {
		targets: {
			"x86_64-apple-darwin":   false
			"x86_64-pc-windows-msv": true
			"x86_64-unknown-linux-gnu": false
		}

		requirements: [
			"""
			This source requires Windows and uses the Windows Event Log API.
			Administrator privileges may be required to access Security channel.
			""",
		]
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: generated.components.sources.windows_eventlog.configuration

	output: logs: {
		event: {
			description: "A Windows Event Log event"
			fields: {
				message: {
					description: "The event message."
					required:    true
					type: string: {
						examples: ["Application started successfully"]
					}
				}
				source_type: {
					description: "The name of the source type."
					required:    true
					type: string: {
						examples: ["windows_eventlog"]
					}
				}
				timestamp: fields._current_timestamp
				event_id: {
					description: "The Windows event ID."
					required:    true
					type: uint: {
						examples: [1000, 4624]
					}
				}
				provider_name: {
					description: "The event provider name."
					required:    false
					type: string: {
						examples: ["Service Control Manager"]
					}
				}
				channel: {
					description: "The event log channel."
					required:    true
					type: string: {
						examples: ["System", "Application"]
					}
				}
			}
		}
	}

	examples: [
		{
			title: "Sample Output"
			configuration: {
				channels: ["System", "Application"]
			}
			input: ""
			output: [{
				log: {
					timestamp:     _values.current_timestamp
					source_type:   "windows_eventlog"
					message:       "The system uptime is 3600 seconds."
					event_id:      6013
					provider_name: "EventLog"
					channel:       "System"
				}
			}]
		},
	]
}

services: windows_eventlog: {
	name:     "Windows Event Log"
	url:      urls.windows_event_log
	versions: null
}
