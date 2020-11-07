package metadata

components: sources: host_metrics: {
	title:       "Host Metrics"
	description: "The host metrics source examines system data sources on the local system and generates metrics describing utilization of various system resources."

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon"]
		development:   "beta"
		egress_method: "batch"
	}

	features: {
		collect: {
			checkpoint: enabled: false
			from: {
				name:     "host"
				thing:    "a \(name)"
				url:      urls.host
				versions: null
			}
		}
		multiline: enabled: false
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		notices: []
		requirements: []
		warnings: []
	}

	configuration: {
		collectors: {
			description: "The list of host metric collector services to use. Defaults to all collectors."
			common:      true
			required:    false
			type: array: {
				default: ["cpu", "disk", "filesystem", "load", "memory", "network"]
				items: type: string: enum: {
					cpu:        "Metrics related to CPU utilization."
					disk:       "Metrics related to disk I/O utilization."
					filesystem: "Metrics related to filesystem space utilization."
					load:       "Load average metrics (UNIX only)."
					memory:     "Metrics related to memory utilization."
					network:    "Metrics related to network utilization."
				}
			}
		}
		namespace: {
			description: "The namespace of metrics. Disabled if empty."
			common:      false
			required:    false
			type: string: default: "host"
		}
		scrape_interval_secs: {
			description: "The interval between metric gathering, in seconds."
			common:      true
			required:    false
			type: uint: {
				default: 15
				unit:    "seconds"
			}
		}
		disk: {
			common:      false
			description: #"Options for the "disk" metrics collector."#
			required:    false
			type: object: options: {
				devices: {
					common:      false
					required:    false
					description: "Lists of device name patterns to include or exclude."
					type: object: options: {
						includes: {
							required: false
							common:   false
							description: """
								The list of device name patterns for which to gather I/O utilization metrics.
								Defaults to including all devices.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: ["*"]
								items: type: string: examples: ["sda", "dm-*"]
							}
						}
						excludes: {
							required: false
							common:   false
							description: """
								The list of device name patterns for which to gather I/O utilization metrics.
								Defaults to excluding no devices.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: []
								items: type: string: examples: ["sda", "dm-*"]
							}
						}
					}
				}
			}
		}
		filesystem: {
			common:      false
			description: #"Options for the "filesystem" metrics collector."#
			required:    false
			type: object: options: {
				devices: {
					common:      false
					required:    false
					description: "Lists of device name patterns to include or exclude."
					type: object: options: {
						includes: {
							required: false
							common:   false
							description: """
								The list of device name patterns for which to gather usage metrics.
								Defaults to including all devices.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: ["*"]
								items: type: string: examples: ["sda", "dm-*"]
							}
						}
						excludes: {
							required: false
							common:   false
							description: """
								The list of device name patterns for which to gather usage metrics.
								Defaults to excluding no devices.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: []
								items: type: string: examples: ["sda", "dm-*"]
							}
						}
					}
				}
				filesystems: {
					common:      false
					required:    false
					description: "Lists of filesystem name patterns to include or exclude."
					type: object: options: {
						includes: {
							required: false
							common:   false
							description: """
								The list of filesystem name patterns for which to gather usage metrics.
								Defaults to including all filesystems.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: ["*"]
								items: type: string: examples: ["ntfs", "ext*"]
							}
						}
						excludes: {
							required: false
							common:   false
							description: """
								The list of filesystem name patterns for which to gather usage metrics.
								Defaults to excluding no filesystems.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: []
								items: type: string: examples: ["ntfs", "ext*"]
							}
						}
					}
				}
				mountpoints: {
					common:      false
					required:    false
					description: "Lists of mount point path patterns to include or exclude."
					type: object: options: {
						includes: {
							required: false
							common:   false
							description: """
								The list of mount point path patterns for which to gather usage metrics.
								Defaults to including all mount points.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: ["*"]
								items: type: string: examples: ["/home", "/raid*"]
							}
						}
						excludes: {
							required: false
							common:   false
							description: """
								The list of mount point path patterns for which to gather usage metrics.
								Defaults to excluding no mount points.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: []
								items: type: string: examples: ["/home", "/raid*"]
							}
						}
					}
				}
			}
		}
		network: {
			common:      false
			description: #"Options for the "network" metrics collector."#
			required:    false
			type: object: options: {
				devices: {
					common:      false
					required:    false
					description: "Lists of device name patterns to include or exclude."
					type: object: options: {
						includes: {
							required: false
							common:   false
							description: """
								The list of device name patterns for which to gather network utilization metrics.
								Defaults to including all devices.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: ["*"]
								items: type: string: examples: ["sda", "dm-*"]
							}
						}
						excludes: {
							required: false
							common:   false
							description: """
								The list of device name patterns for which to gather network utilization metrics.
								Defaults to excluding no devices.
								The patterns are matched using [globbing](#globbing).
								"""
							type: array: {
								default: []
								items: type: string: examples: ["sda", "dm-*"]
							}
						}
					}
				}
			}
		}
	}

	output: metrics: _host_metrics
}
