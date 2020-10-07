package metadata

#IncludesExcludes: {
	common:      false
	description: "Lists of \(#term) patterns to include or exclude."
	required:    false
	type: object: options: {
		includes: {
			description: #"""
				The list of \(#term) patterns for which to gather \(#metrics) metrics.
				Defaults to including all \(#term)s.
				The patterns are matched using [globbing](#globbing).
				"""#
			type: "[string]"
		}
		excludes: {
			description: #"""
				The list of \(#term) patterns for which to skip gathering \(#metrics) metrics.
				Defaults to excluding no \(#term)s.
				The patterns are matched using [globbing](#globbing).
				"""#
			type: "[string]"
		}
	}
}

components: sources: host_metrics: {
	title:             "Host Metrics"
	long_description:  "FIXME"
	short_description: "Gather host-based metrics."

	classes: {
		commonly_used: false
		deployment_roles: ["daemon"]
		function: "collect"
	}

	features: {
		checkpoint: enabled: false
		multiline: enabled:  false
		tls: enabled:        false
	}

	statuses: {
		delivery:    "at_least_once"
		development: "beta"
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

		requirements: [
		]

		warnings: [
		]
	}

	configuration: {
		collectors: {
			description: "The list of host metric collector services to use. Defaults to all collectors."
			common:      true
			required:    false
			type: "[string]": {
				default: ["cpu", "disk", "filesystem", "load", "memory", "network"]
				enum: {
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
			description: "The namespace prefix that will be added to all metric names."
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
				devices: #IncludesExcludes & {
					term:    "device name"
					metrics: "I/O utilization"
					type: object: options: {
						includes: examples: [["sda"], ["dm-*"]]
						excludes: examples: [["sda"], ["dm-*"]]
					}
				}
			}
		}
		filesystem: {
			common:      false
			description: #"Options for the "filesystem" metrics collector."#
			required:    false
			type: object: options: {
				devices: #IncludesExcludes & {
					term:    "device name"
					metrics: "usage"
					type: object: options: {
						includes: examples: [["sda"], ["dm-*"]]
						excludes: examples: [["sda"], ["dm-*"]]
					}
				}
				filesystems: #IncludesExcludes & {
					term:    "filesystem name"
					metrics: "usage"
					type: object: options: {
						includes: examples: [["ntfs"], ["ext*"]]
						excludes: examples: [["ntfs"], ["ext*"]]
					}
				}
				mountpoints: #IncludesExcludes & {
					term:    "mount point path"
					metrics: "usage"
					type: object: options: {
						includes: examples: [["/home"], ["/raid*"]]
						excludes: examples: [["/home"], ["/raid*"]]
					}
				}
			}
		}
		network: {
			common:      false
			description: #"Options for the "network" metrics collector."#
			required:    false
			type: object: options: {
				devices: #IncludesExcludes & {
					term:    "device name"
					metrics: "utilization"
					type: object: options: {
						includes: examples: [["eth0"], ["enp?s*"]]
						excludes: examples: [["eth0"], ["enp?s*"]]
					}
				}
			}
		}
	}

	output: metrics: {
	}

	how_it_works: {
	}
}
