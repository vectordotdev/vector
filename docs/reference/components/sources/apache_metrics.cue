package metadata

components: sources: apache_metrics: {
	title:             "Apache HTTPD Metrics"
	long_description:  "fill me in"
	short_description: "Collect metrics from an Apache HTTPD server."

	classes: {
		commonly_used: false
		deployment_roles: ["daemon", "sidecar"]
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
			"""
				The Apache [Status module (`mod_status`)][urls.apache_mod_status] must
				enabled and configured for this source to work.
				""",
		]

		warnings: [
			"""
				The [`ExtendedStatus` option][urls.apache_extended_status] has been known to
				cause performance problems. If enabled, please monitor performance
				carefully.
				""",
		]
	}

	configuration: {
		endpoints: {
			description: "mod_status endpoints to scrape metrics from."
			required:    true
			type: "[string]": {
				examples: [["http://localhost:8080/server-status/?auto"]]
			}
		}
		interval_secs: {
			description: "The interval between scrapes."
			common:      true
			required:    false
			type: uint: {
				default: 15
				unit:    "seconds"
			}
		}
	}

	output: metrics: {
		_endpoint: {
			description: "The absolute path of originating file."
			required:    true
			examples: ["http://localhost:8080/server-status?auto"]
		}
		_host: {
			description: "The hostname of the Apache HTTP server"
			required:    true
			examples: ["localhost"]
		}
		apache_access_total: {
			description:   "The total number of time the Apache server has been accessed."
			relevant_when: "`ExtendedStatus On`"
			type:          "counter"
			tags: {
				endpoint: _endpoint
				host:     _host
			}
		}
		apache_connections: {
			description: "The total number of time the Apache server has been accessed."
			type:        "gauge"
			tags: {
				endpoint: _endpoint
				host:     _host
				state: {
					description: "The state of the connection"
					required:    true
					examples: ["closing", "keepalive", "total", "writing"]
				}
			}
		}
		apache_cpu_load: {
			description:   "The current CPU of the Apache server."
			relevant_when: "`ExtendedStatus On`"
			type:          "gauge"
			tags: {
				endpoint: _endpoint
				host:     _host
			}
		}
		apache_cpu_seconds_total: {
			description:   "The CPU time of various Apache processes."
			relevant_when: "`ExtendedStatus On`"
			type:          "counter"
			tags: {
				endpoint: _endpoint
				host:     _host
				type: {
					description: "The state of the connection"
					required:    true
					examples: ["children_system", "children_user", "system", "user"]
				}
			}
		}
		apache_duration_seconds_total: {
			description:   "The amount of time the Apache server has been running."
			relevant_when: "`ExtendedStatus On`"
			type:          "counter"
			tags: {
				endpoint: _endpoint
				host:     _host
			}
		}
		apache_scoreboard: {
			description: "The amount of times various Apache server tasks have been run."
			type:        "gauge"
			tags: {
				endpoint: _endpoint
				host:     _host
				state: {
					description: "The connect state"
					required:    true
					examples: ["closing", "dnslookup", "finishing", "idle_cleanup", "keepalive", "logging", "open", "reading", "sending", "starting", "waiting"]
				}
			}
		}
		apache_sent_bytes_total: {
			description:   "The amount of bytes sent by the Apache server."
			relevant_when: "`ExtendedStatus On`"
			type:          "counter"
			tags: {
				endpoint: _endpoint
				host:     _host
			}
		}
		apache_uptime_seconds_total: {
			description: "The amount of time the Apache server has been running."
			type:        "counter"
			tags: {
				endpoint: _endpoint
				host:     _host
			}
		}
		apache_workers: {
			description: "Apache worker statuses."
			type:        "gauge"
			tags: {
				endpoint: _endpoint
				host:     _host
				state: {
					description: "The state of the worker"
					required:    true
					examples: ["busy", "idle"]
				}
			}
		}
		apache_up: {
			description: "If the Apache server is up or not."
			type:        "gauge"
			tags: {
				endpoint: _endpoint
				host:     _host
			}
		}
	}

	how_it_works: {
		mod_status: {
			title: "Apache Status Module (mod_status)"
			body: #"""
				This source works by scraping the configured
				[Apache Status module][urls.apache_mod_status] endpoint
				which exposes basic metrics about Apache's runtime.
				"""#
			sub_sections: [
				{
					title: "Extended Status"
					body: #"""
						The Apache Status module offers an
						[`ExtendedStatus` directive][urls.apache_extended_status]
						that includes additional detailed runtime metrics with
						your configured `mod_status` endpoint. Vector will
						recognize these metrics and expose them accordingly.
						"""#
				},
			]
		}
	}
}
