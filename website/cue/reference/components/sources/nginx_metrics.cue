package metadata

components: sources: nginx_metrics: {
	title: "Nginx Metrics"

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
		collect: {
			checkpoint: enabled: false
			from: {
				service: services.nginx

				interface: {
					socket: {
						api: {
							title: "Nginx ngx_http_stub_status_module module"
							url:   urls.nginx_stub_status_module
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "optional"
					}
				}
			}
			proxy: enabled: true
		}
		multiline: enabled: false
	}

	support: {
		requirements: [
			"Module `ngx_http_stub_status_module` should be enabled.",
		]

		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.nginx_metrics.configuration

	how_it_works: {
		mod_status: {
			title: "Module `ngx_http_stub_status_module`"
			body:  """
				The [ngx_http_stub_status_module](\(urls.nginx_stub_status_module))
				module provides access to basic status information. Basic status
				information is a simple web page with text data.
				"""
		}
	}

	output: metrics: {
		// Default Nginx tags
		_nginx_metrics_tags: {
			endpoint: {
				description: "Nginx endpoint."
				required:    true
				examples: ["http://localhost:8000/basic_status"]
			}
			host: {
				description: "The hostname of the Nginx server."
				required:    true
				examples: [_values.local_host]
			}
		}

		up: {
			description:       "If the Nginx server is up or not."
			type:              "gauge"
			default_namespace: "nginx"
			tags:              _nginx_metrics_tags
		}
		connections_active: {
			description:       "The current number of active client connections including `Waiting` connections."
			type:              "gauge"
			default_namespace: "nginx"
			tags:              _nginx_metrics_tags
		}
		connections_accepted_total: {
			description:       "The total number of accepted client connections."
			type:              "counter"
			default_namespace: "nginx"
			tags:              _nginx_metrics_tags
		}
		connections_handled_total: {
			description:       "The total number of handled connections. Generally, the parameter value is the same as `accepts` unless some resource limits have been reached (for example, the `worker_connections` limit)."
			type:              "counter"
			default_namespace: "nginx"
			tags:              _nginx_metrics_tags
		}
		http_requests_total: {
			description:       "The total number of client requests."
			type:              "counter"
			default_namespace: "nginx"
			tags:              _nginx_metrics_tags
		}
		connections_reading: {
			description:       "The current number of connections where nginx is reading the request header."
			type:              "gauge"
			default_namespace: "nginx"
			tags:              _nginx_metrics_tags
		}
		connections_writing: {
			description:       "The current number of connections where nginx is writing the response back to the client."
			type:              "gauge"
			default_namespace: "nginx"
			tags:              _nginx_metrics_tags
		}
		connections_waiting: {
			description:       "The current number of idle client connections waiting for a request."
			type:              "gauge"
			default_namespace: "nginx"
			tags:              _nginx_metrics_tags
		}
	}

	telemetry: metrics: {
		collect_completed_total:  components.sources.internal_metrics.output.metrics.collect_completed_total
		collect_duration_seconds: components.sources.internal_metrics.output.metrics.collect_duration_seconds
	}
}
