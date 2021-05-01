package metadata

configuration: {
	configuration: #Schema
	how_it_works:  #HowItWorks
}

configuration: {
	configuration: {
		data_dir: {
			common: false
			description: """
				The directory used for persisting Vector state, such
				as on-disk buffers, file checkpoints, and more.
				Please make sure the Vector project has write
				permissions to this directory.
				"""
			required: false
			type: string: {
				default: "/var/lib/vector/"
				examples: ["/var/lib/vector", "/var/local/lib/vector/", "/home/user/vector/"]
				syntax: "literal"
			}
		}

		log_schema: {
			common: false
			description: """
				Configures default log schema for all events. This is used by
				Vector source components to assign the fields on incoming
				events.
				"""
			required: false
			warnings: []
			type: object: {
				examples: []
				options: {
					message_key: {
						common: true
						description: """
							Sets the event key to use for the event message field.
							"""
						required: false
						warnings: ["This option is deprecated in-lieu of using [`remap` transform](\(urls.vector_remap_transform)) to rename fields"]
						type: string: {
							default: "message"
							examples: ["message", "@message"]
							syntax: "literal"
						}
					}

					timestamp_key: {
						common: true
						description: """
							Sets the event key to use for the event timestamp field.
							"""
						required: false
						warnings: ["This option is deprecated in-lieu of using [`remap` transform](\(urls.vector_remap_transform)) to rename fields"]
						type: string: {
							default: "timestamp"
							examples: ["timestamp", "@timestamp"]
							syntax: "literal"
						}
					}

					host_key: {
						common: true
						description: """
							Sets the event key to use for the event host field.
							"""
						required: false
						warnings: ["This option is deprecated in-lieu of using [`remap` transform](\(urls.vector_remap_transform)) to rename fields"]
						type: string: {
							default: "host"
							examples: ["host", "@host"]
							syntax: "literal"
						}
					}

					source_type_key: {
						common: true
						description: """
							Sets the event key to use for the event source type
							field that is set by some sources.
							"""
						required: false
						warnings: ["This option is deprecated in-lieu of using [`remap` transform](\(urls.vector_remap_transform)) to rename fields"]
						type: string: {
							default: "source_type"
							examples: ["source_type", "@source_type"]
							syntax: "literal"
						}
					}
				}
			}
		}

		healthchecks: {
			common: false
			description: """
				Configures health checks for all sinks.
				"""
			required: false
			warnings: []
			type: object: {
				examples: []
				options: {
					enabled: {
						common: true
						description: """
							Disables all health checks if false, otherwise sink specific
							option overrides it.
							"""
						required: false
						warnings: []
						type: bool: {
							default: true
						}
					}

					require_healthy: {
						common: false
						description: """
							Exit on startup if any sinks' health check fails. Overridden by
							`--require-healthy` command line flag.
							"""
						required: false
						warnings: []
						type: bool: {
							default: false
						}
					}
				}
			}
		}

		timezone: {
			common:      false
			description: """
				The name of the time zone to apply to timestamp conversions that do not contain an
				explicit time zone. The time zone name may be any name in the
				[TZ database](\(urls.tz_time_zones)), or `local` to indicate system local time.
				"""
			required:    false
			warnings: []
			type: string: {
				default: "local"
				examples: ["local", "America/NewYork", "EST5EDT"]
				syntax: "literal"
			}
		}
	}
}
