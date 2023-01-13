package metadata

base: components: sources: host_metrics: configuration: {
	collectors: {
		description: """
			The list of host metric collector services to use.

			Defaults to all collectors.
			"""
		required: false
		type: array: items: type: string: {
			enum: {
				cpu:        "Metrics related to CPU utilization."
				disk:       "Metrics related to disk I/O utilization."
				filesystem: "Metrics related to filesystem space utilization."
				host:       "Metrics related to the host."
				load:       "Load average metrics."
				memory:     "Metrics related to memory utilization."
				network:    "Metrics related to network utilization."
			}
			examples: ["cgroups", "cpu", "disk", "filesystem", "load", "host", "memory", "network"]
		}
	}
	disk: {
		description: "Options for the “disk” metrics collector."
		required:    false
		type: object: options: devices: {
			description: """
				Lists of device name patterns to include or exclude in gathering
				I/O utilization metrics.

				Defaults to including all devices.
				"""
			required: false
			type: object: {
				examples: [{
					excludes: ["dm-*"]
					includes: ["sda"]
				}]
				options: {
					excludes: {
						description: """
																Any patterns which should be excluded.

																The patterns are matched using globbing.
																"""
						required: false
						type: array: items: type: string: {}
					}
					includes: {
						description: """
																Any patterns which should be included.

																The patterns are matched using globbing.
																"""
						required: false
						type: array: items: type: string: {}
					}
				}
			}
		}
	}
	filesystem: {
		description: "Options for the “filesystem” metrics collector."
		required:    false
		type: object: options: {
			devices: {
				description: """
					Lists of device name patterns to include or exclude in gathering
					usage metrics.

					Defaults to including all devices.
					"""
				required: false
				type: object: {
					examples: [{
						excludes: ["dm-*"]
						includes: ["sda"]
					}]
					options: {
						excludes: {
							description: """
																Any patterns which should be excluded.

																The patterns are matched using globbing.
																"""
							required: false
							type: array: items: type: string: {}
						}
						includes: {
							description: """
																Any patterns which should be included.

																The patterns are matched using globbing.
																"""
							required: false
							type: array: items: type: string: {}
						}
					}
				}
			}
			filesystems: {
				description: """
					Lists of filesystem name patterns to include or exclude in gathering
					usage metrics.

					Defaults to including all filesystems.
					"""
				required: false
				type: object: {
					examples: [{
						excludes: ["ext*"]
						includes: ["ntfs"]
					}]
					options: {
						excludes: {
							description: """
																Any patterns which should be excluded.

																The patterns are matched using globbing.
																"""
							required: false
							type: array: items: type: string: {}
						}
						includes: {
							description: """
																Any patterns which should be included.

																The patterns are matched using globbing.
																"""
							required: false
							type: array: items: type: string: {}
						}
					}
				}
			}
			mountpoints: {
				description: """
					Lists of mount point path patterns to include or exclude in gathering
					usage metrics.

					Defaults to including all mount points.
					"""
				required: false
				type: object: {
					examples: [{
						excludes: ["/raid*"]
						includes: ["/home"]
					}]
					options: {
						excludes: {
							description: """
																Any patterns which should be excluded.

																The patterns are matched using globbing.
																"""
							required: false
							type: array: items: type: string: {}
						}
						includes: {
							description: """
																Any patterns which should be included.

																The patterns are matched using globbing.
																"""
							required: false
							type: array: items: type: string: {}
						}
					}
				}
			}
		}
	}
	namespace: {
		description: "Overrides the default namespace for the metrics emitted by the source."
		required:    false
		type: string: default: "host"
	}
	network: {
		description: "Options for the “network” metrics collector."
		required:    false
		type: object: options: devices: {
			description: """
				Lists of device name patterns to include or exclude in gathering
				network utilization metrics.

				Defaults to including all devices.
				"""
			required: false
			type: object: {
				examples: [{
					excludes: ["dm-*"]
					includes: ["sda"]
				}]
				options: {
					excludes: {
						description: """
																Any patterns which should be excluded.

																The patterns are matched using globbing.
																"""
						required: false
						type: array: items: type: string: {}
					}
					includes: {
						description: """
																Any patterns which should be included.

																The patterns are matched using globbing.
																"""
						required: false
						type: array: items: type: string: {}
					}
				}
			}
		}
	}
	scrape_interval_secs: {
		description: "The interval between metric gathering, in seconds."
		required:    false
		type: uint: {
			default: 15
			unit:    "seconds"
		}
	}
}
