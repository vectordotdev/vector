package metadata

base: components: sources: host_metrics: configuration: {
	collectors: {
		description: """
			The list of host metric collector services to use.

			Defaults to all collectors.
			"""
		required: false
		type: array: items: type: string: enum: {
			cpu:        "CPU."
			disk:       "Disk."
			filesystem: "Filesystem."
			host:       "Host."
			load:       "Load average."
			memory:     "Memory."
			network:    "Network."
		}
	}
	disk: {
		description: "Options for the “disk” metrics collector."
		required:    false
		type: object: options: devices: {
			description: "Lists of device name patterns to include or exclude."
			required:    false
			type: object: {
				default: {
					excludes: null
					includes: null
				}
				options: {
					excludes: {
						description: "Any patterns which should be excluded."
						required:    false
						type: array: items: type: string: syntax: "literal"
					}
					includes: {
						description: "Any patterns which should be included."
						required:    false
						type: array: items: type: string: syntax: "literal"
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
				description: "Lists of device name patterns to include or exclude."
				required:    false
				type: object: {
					default: {
						excludes: null
						includes: null
					}
					options: {
						excludes: {
							description: "Any patterns which should be excluded."
							required:    false
							type: array: items: type: string: syntax: "literal"
						}
						includes: {
							description: "Any patterns which should be included."
							required:    false
							type: array: items: type: string: syntax: "literal"
						}
					}
				}
			}
			filesystems: {
				description: "Lists of filesystem name patterns to include or exclude."
				required:    false
				type: object: {
					default: {
						excludes: null
						includes: null
					}
					options: {
						excludes: {
							description: "Any patterns which should be excluded."
							required:    false
							type: array: items: type: string: syntax: "literal"
						}
						includes: {
							description: "Any patterns which should be included."
							required:    false
							type: array: items: type: string: syntax: "literal"
						}
					}
				}
			}
			mountpoints: {
				description: "Lists of mount point path patterns to include or exclude."
				required:    false
				type: object: {
					default: {
						excludes: null
						includes: null
					}
					options: {
						excludes: {
							description: "Any patterns which should be excluded."
							required:    false
							type: array: items: type: string: syntax: "literal"
						}
						includes: {
							description: "Any patterns which should be included."
							required:    false
							type: array: items: type: string: syntax: "literal"
						}
					}
				}
			}
		}
	}
	namespace: {
		description: """
			Overrides the default namespace for the metrics emitted by the source.

			By default, `host` is used.
			"""
		required: false
		type: string: {
			default: "host"
			syntax:  "literal"
		}
	}
	network: {
		description: "Options for the “network” metrics collector."
		required:    false
		type: object: options: devices: {
			description: "Lists of device name patterns to include or exclude."
			required:    false
			type: object: {
				default: {
					excludes: null
					includes: null
				}
				options: {
					excludes: {
						description: "Any patterns which should be excluded."
						required:    false
						type: array: items: type: string: syntax: "literal"
					}
					includes: {
						description: "Any patterns which should be included."
						required:    false
						type: array: items: type: string: syntax: "literal"
					}
				}
			}
		}
	}
	scrape_interval_secs: {
		description: "The interval between metric gathering, in seconds."
		required:    false
		type: float: default: 15.0
	}
}
