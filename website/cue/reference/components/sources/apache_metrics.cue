package metadata

components: sources: apache_metrics: {
	title: "Apache HTTP Server (HTTPD) Metrics"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "sidecar"]
		development:   "stable"
		egress_method: "batch"
		stateful:      false
	}

	features: {
		acknowledgements: false
		multiline: enabled: false
		collect: {
			checkpoint: enabled: false
			from: {
				service: services.apache_http

				interface: {
					socket: {
						api: {
							title: "Apache HTTP Server Status Module"
							url:   urls.apache_mod_status
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "disabled"
					}
				}
			}
			proxy: enabled: true
		}
	}

	support: {
		requirements: [
			"""
			The [Apache Status module](\(urls.apache_mod_status)) must be enabled.
			""",
		]
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.apache_metrics.configuration

	output: metrics: {
		// Default Apache metrics tags
		_apache_metrics_tags: {
			endpoint: {
				description: "The absolute path of originating file."
				required:    true
				examples: ["http://localhost:8080/server-status?auto"]
			}
			host: {
				description: "The hostname of the Apache HTTP server."
				required:    true
				examples: [_values.local_host]
			}
		}

		access_total: {
			description:       "The total number of time the Apache server has been accessed."
			relevant_when:     "`ExtendedStatus On`"
			type:              "counter"
			default_namespace: "apache"
			tags:              _apache_metrics_tags
		}
		connections: {
			description:       "The total number of time the Apache server has been accessed."
			type:              "gauge"
			default_namespace: "apache"
			tags: _apache_metrics_tags & {
				state: {
					description: "The state of the connection"
					required:    true
					examples: ["closing", "keepalive", "total", "writing"]
				}
			}
		}
		cpu_load: {
			description:       "The current CPU of the Apache server."
			relevant_when:     "`ExtendedStatus On`"
			type:              "gauge"
			default_namespace: "apache"
			tags:              _apache_metrics_tags
		}
		cpu_seconds_total: {
			description:       "The CPU time of various Apache processes."
			relevant_when:     "`ExtendedStatus On`"
			type:              "counter"
			default_namespace: "apache"
			tags: _apache_metrics_tags & {
				state: {
					description: "The state of the connection"
					required:    true
					examples: ["children_system", "children_user", "system", "user"]
				}
			}
		}
		duration_seconds_total: {
			description:       "The amount of time the Apache server has been running."
			relevant_when:     "`ExtendedStatus On`"
			type:              "counter"
			default_namespace: "apache"
			tags:              _apache_metrics_tags
		}
		scoreboard: {
			description:       "The amount of times various Apache server tasks have been run."
			type:              "gauge"
			default_namespace: "apache"
			tags: _apache_metrics_tags & {
				state: {
					description: "The connect state"
					required:    true
					examples: ["closing", "dnslookup", "finishing", "idle_cleanup", "keepalive", "logging", "open", "reading", "sending", "starting", "waiting"]
				}
			}
		}
		sent_bytes_total: {
			description:       "The amount of bytes sent by the Apache server."
			relevant_when:     "`ExtendedStatus On`"
			type:              "counter"
			default_namespace: "apache"
			tags:              _apache_metrics_tags
		}
		up: {
			description:       "If the Apache server is up or not."
			type:              "gauge"
			default_namespace: "apache"
			tags:              _apache_metrics_tags
		}
		uptime_seconds_total: {
			description:       "The amount of time the Apache server has been running."
			type:              "counter"
			default_namespace: "apache"
			tags:              _apache_metrics_tags
		}
		workers: {
			description:       "Apache worker statuses."
			type:              "gauge"
			default_namespace: "apache"
			tags: _apache_metrics_tags & {
				state: {
					description: "The state of the worker"
					required:    true
					examples: ["busy", "idle"]
				}
			}
		}
	}

	how_it_works: {}

	telemetry: metrics: {
		http_client_responses_total:      components.sources.internal_metrics.output.metrics.http_client_responses_total
		http_client_response_rtt_seconds: components.sources.internal_metrics.output.metrics.http_client_response_rtt_seconds
	}
}
