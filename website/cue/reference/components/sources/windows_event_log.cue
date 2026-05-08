package metadata

components: sources: windows_event_log: {
	title: "Windows Event Log"

	description: """
		Collects log events from Windows Event Log channels using the native
		Windows Event Log API.
		"""

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon"]
		development:   "beta"
		egress_method: "stream"
		stateful:      true
	}

	features: {
		auto_generated:   true
		acknowledgements: true
		collect: {
			checkpoint: enabled: true
			from: service: {
				name:     "Windows Event Log"
				thing:    "Windows Event Log channels"
				url:      "https://learn.microsoft.com/en-us/windows/win32/wes/windows-event-log"
				versions: null
			}
		}
		multiline: enabled: false
	}

	support: {
		requirements: [
			"""
				This source is only supported on Windows. Attempting to use it on
				other operating systems will result in an error at startup.
				""",
		]
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: generated.components.sources.windows_event_log.configuration

	output: {
		logs: event: {
			description: "An individual Windows Event Log event."
			fields: {
				source_type: {
					description: "The name of the source type."
					required:    true
					type: string: {
						examples: ["windows_event_log"]
					}
				}
				timestamp: {
					description: "The timestamp of the event."
					required:    false
					type: timestamp: {}
				}
				message: {
					description: "The rendered event message."
					required:    false
					type: string: {
						examples: ["The service was started successfully."]
					}
				}
				channel: {
					description: "The event log channel name."
					required:    false
					type: string: {
						examples: ["System", "Application", "Security"]
					}
				}
				event_id: {
					description: "The event identifier."
					required:    false
					type: uint: {
						examples: [7036, 4624, 1000]
					}
				}
				provider_name: {
					description: "The name of the event provider."
					required:    false
					type: string: {
						examples: ["Microsoft-Windows-Security-Auditing"]
					}
				}
				computer: {
					description: "The name of the computer that generated the event."
					required:    false
					type: string: {
						examples: ["DESKTOP-ABC123"]
					}
				}
				level: {
					description: "The event severity level."
					required:    false
					type: string: {
						examples: ["Information", "Warning", "Error", "Critical"]
					}
				}
			}
		}
	}
}
