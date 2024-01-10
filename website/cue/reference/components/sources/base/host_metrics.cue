package metadata

base: components: sources: host_metrics: configuration: {
	cgroups: {
		description: """
			Options for the cgroups (controller groups) metrics collector.

			This collector is only available on Linux systems, and only supports either version 2 or hybrid cgroups.
			"""
		required: false
		type: object: options: {
			base: {
				description: "The base cgroup name to provide metrics for."
				required:    false
				type: string: examples: ["/", "system.slice/snapd.service"]
			}
			groups: {
				description: """
					Lists of cgroup name patterns to include or exclude in gathering
					usage metrics.
					"""
				required: false
				type: object: {
					examples: [{
						excludes: ["*.service"]
						includes: ["user.slice/*"]
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
							type: array: {
								default: ["*"]
								items: type: string: {}
							}
						}
					}
				}
			}
			levels: {
				description: """
					The number of levels of the cgroups hierarchy for which to report metrics.

					A value of `1` means the root or named cgroup.
					"""
				required: false
				type: uint: {
					default: 100
					examples: [1, 3]
				}
			}
		}
	}
	collectors: {
		description: """
			The list of host metric collector services to use.

			Defaults to all collectors.
			"""
		required: false
		type: array: {
			default: ["cpu", "disk", "filesystem", "load", "host", "memory", "network", "cgroups"]
			items: type: string: {
				enum: {
					cgroups: """
						Metrics related to Linux control groups.

						Only available on Linux.
						"""
					cpu:        "Metrics related to CPU utilization."
					disk:       "Metrics related to disk I/O utilization."
					filesystem: "Metrics related to filesystem space utilization."
					host:       "Metrics related to the host."
					load:       "Metrics related to the system load average."
					memory:     "Metrics related to memory utilization."
					network:    "Metrics related to network utilization."
				}
				examples: ["cgroups", "cpu", "disk", "filesystem", "load", "host", "memory", "network"]
			}
		}
	}
	disk: {
		description: "Options for the disk metrics collector."
		required:    false
		type: object: options: devices: {
			description: """
				Lists of device name patterns to include or exclude in gathering
				I/O utilization metrics.
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
						type: array: {
							default: ["*"]
							items: type: string: {}
						}
					}
				}
			}
		}
	}
	filesystem: {
		description: "Options for the filesystem metrics collector."
		required:    false
		type: object: options: {
			devices: {
				description: """
					Lists of device name patterns to include or exclude in gathering
					usage metrics.
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
							type: array: {
								default: ["*"]
								items: type: string: {}
							}
						}
					}
				}
			}
			filesystems: {
				description: """
					Lists of filesystem name patterns to include or exclude in gathering
					usage metrics.
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
							type: array: {
								default: ["*"]
								items: type: string: {}
							}
						}
					}
				}
			}
			mountpoints: {
				description: """
					Lists of mount point path patterns to include or exclude in gathering
					usage metrics.
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
							type: array: {
								default: ["*"]
								items: type: string: {}
							}
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
		description: "Options for the network metrics collector."
		required:    false
		type: object: options: devices: {
			description: """
				Lists of device name patterns to include or exclude in gathering
				network utilization metrics.
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
						type: array: {
							default: ["*"]
							items: type: string: {}
						}
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
