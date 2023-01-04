package metadata

components: sinks: prometheus_remote_write: {
	title: "Prometheus Remote Write"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["AWS"]
		stateful: false
	}

	features: {
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_events:   1000
				timeout_secs: 1.0
			}
			// TODO Snappy is always enabled
			compression: enabled: false
			encoding: enabled:    false
			proxy: enabled:       true
			request: {
				enabled:                    true
				rate_limit_duration_secs:   1
				rate_limit_num:             5
				retry_initial_backoff_secs: 1
				retry_max_duration_secs:    10
				timeout_secs:               60
				headers:                    false
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
				enabled_by_scheme:      true
			}
			to: {
				service: services.prometheus_remote_write

				interface: {
					socket: {
						api: {
							title: "Prometheus remote_write protocol"
							url:   urls.prometheus_remote_write
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "optional"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: [
			"""
				High cardinality metric names and labels are discouraged by
				Prometheus as they can provide performance and reliability
				problems. You should consider alternative strategies to reduce
				the cardinality. Vector offers a [`tag_cardinality_limit`
				transform](\(urls.vector_transforms)/tag_cardinality_limit)
				as a way to protect against this.
				""",
		]
		notices: []
	}

	configuration: {
		endpoint: {
			description: "The endpoint URL to send data to."
			required:    true
			warnings: []
			type: string: {
				examples: ["https://localhost:8087/"]
			}
		}
		auth: {
			common:      false
			description: "Options for authentication."
			required:    false
			type: object: {
				examples: []
				options: components._aws.configuration.auth.type.object.options & {
					password: {
						description: "The basic authentication password."
						required:    true
						type: string: {
							examples: ["${PROMETHEUS_PASSWORD}", "password"]
						}
					}
					strategy: {
						description: "The authentication strategy to use."
						required:    true
						type: string: {
							enum: {
								aws:   "Authentication strategy used for the [AWS managed Prometheus service](\(urls.aws_prometheus))."
								basic: "The [basic authentication strategy](\(urls.basic_auth))."
							}
						}
					}
					user: {
						description: "The basic authentication user name."
						required:    true
						type: string: {
							examples: ["${PROMETHEUS_USERNAME}", "username"]
						}
					}
				}
			}
		}
		aws: {
			common:      false
			description: "Options for the AWS connections."
			required:    false
			type: object: {
				examples: []
				options: {
					region: {
						common:      true
						description: "The [AWS region](\(urls.aws_regions)) of the target service. This defaults to the region named in the endpoint parameter, or the value of the `$AWS_REGION` or `$AWS_DEFAULT_REGION` environment variables if that cannot be determined, or \"us-east-1\"."
						required:    false
						type: string: {
							default: null
							examples: ["us-east-1"]
						}
					}
				}
			}
		}
		default_namespace: {
			common:      true
			description: """
				Used as a namespace for metrics that don't have it.
				A namespace will be prefixed to a metric's name.
				It should follow Prometheus [naming conventions](\(urls.prometheus_metric_naming)).
				"""
			required:    false
			type: string: {
				default: null
				examples: ["service"]
			}
		}
		buckets: {
			common:      false
			description: "Default buckets to use for aggregating [distribution](\(urls.vector_metric)/#distribution) metrics into histograms."
			required:    false
			type: array: {
				default: [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
				items: type: float: examples: [0.005, 0.01]
			}
		}
		quantiles: {
			common:      false
			description: "Quantiles to use for aggregating [distribution](\(urls.vector_metric)/#distribution) metrics into a summary."
			required:    false
			type: array: {
				default: [0.5, 0.75, 0.9, 0.95, 0.99]
				items: type: float: examples: [0.5, 0.75, 0.9, 0.95, 0.99]
			}
		}
		tenant_id: {
			common:      false
			description: "If set, a header named `X-Scope-OrgID` will be added to outgoing requests with the text of this setting. This may be used by Cortex or other remote services to identify the tenant making the request."
			required:    false
			type: string: {
				default: null
				examples: ["my-domain"]
				syntax: "template"
			}
		}
	}

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          false
			summary:      true
		}
		traces: false
	}

	how_it_works: {
		duplicate_tag_names: {
			title: "Duplicate tag names"
			body: """
				Multiple tags with the same name are invalid within Prometheus and Prometheus
				will reject a metric with duplicate tag names. When sending a tag with multiple
				values for each name, Vector will only send the last value specified.
				"""
		}
	}

	telemetry: metrics: {
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		processing_errors_total:          components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
