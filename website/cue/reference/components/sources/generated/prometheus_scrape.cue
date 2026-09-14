package metadata

generated: components: sources: prometheus_scrape: configuration: {
	auth: {
		description: """
			Configuration of the authentication strategy for HTTP requests.

			HTTP authentication should be used with HTTPS only, as the authentication credentials are passed as an
			HTTP header without any additional encryption beyond what is provided by the transport itself.
			"""
		required: false
		type:     _schemaDefinitions["core::option::Option<vector::http::Auth>"]
	}
	endpoint_tag: {
		description: """
			The tag name added to each event representing the scraped instance's endpoint.

			The tag value is the endpoint of the scraped instance.
			"""
		required: false
		type: string: {}
	}
	endpoints: {
		description: """
			Endpoints to scrape metrics from.

			Deprecated: use `targets` with a `static` block instead.
			"""
		required: false
		type: array: {
			default: []
			items: type: string: examples: ["http://localhost:9090/metrics"]
		}
	}
	honor_labels: {
		description: """
			Controls how tag conflicts are handled if the scraped source has tags to be added.

			If `true`, the new tag is not added if the scraped metric has the tag already. If `false`, the conflicting tag
			is renamed by prepending `exported_` to the original name.

			This matches Prometheus' `honor_labels` configuration.
			"""
		required: false
		type: bool: default: false
	}
	instance_tag: {
		description: """
			The tag name added to each event representing the scraped instance's `host:port`.

			The tag value is the host and port of the scraped instance.
			"""
		required: false
		type: string: {}
	}
	query: {
		description: """
			Custom parameters for the scrape request query string.

			One or more values for the same parameter key can be provided. The parameters provided in this option are
			appended to any parameters manually provided in the `endpoints` option. This option is especially useful when
			scraping the `/federate` endpoint.
			"""
		required: false
		type: object: {
			examples: [{
				"match[]": ["{job=\"somejob\"}", "{__name__=~\"job:.*\"}"]
			}]
			options: "*": {
				description: "A query string parameter."
				required:    true
				type:        _schemaDefinitions["vector::http::ParameterValue"]
			}
		}
	}
	scrape_interval_secs: {
		description: """
			The interval between scrapes. Requests are run concurrently so if a scrape takes longer
			than the interval a new scrape will be started. This can take extra resources, set the timeout
			to a value lower than the scrape interval to prevent this from happening.
			"""
		required: false
		type: uint: {
			default: 15
			unit:    "seconds"
		}
	}
	scrape_timeout_secs: {
		description: "The timeout for each scrape request."
		required:    false
		type: float: {
			default: 5.0
			unit:    "seconds"
		}
	}
	targets: {
		description: """
			Auto-discover scrape targets.

			Each entry in the list configures a target group. Supported types are
			`static` (fixed URLs) and `kubernetes` (Pod annotation discovery).
			"""
		required: false
		type: array: {
			default: []
			items: type: object: options: {
				kubernetes: {
					description: "Kubernetes Pod auto-discovery via `prometheus.io/*` annotations."
					required:    false
					type: object: options: {
						annotation_prefix: {
							description: """
																			Annotation prefix to read scrape configuration from.

																			Defaults to `prometheus.io`, matching the de-facto Prometheus convention.
																			The source reads `<prefix>/scrape`, `<prefix>/port`, `<prefix>/path`,
																			`<prefix>/scheme`, and `<prefix>/param_<name>` from Pod annotations.
																			"""
							required: false
							type: string: {
								default: "prometheus.io"
								examples: ["prometheus.io"]
							}
						}
						default_path: {
							description: "Default scrape path when `<prefix>/path` is not set on a Pod."
							required:    false
							type: string: {
								default: "/metrics"
								examples: ["/metrics"]
							}
						}
						default_scheme: {
							description: "Default scheme when `<prefix>/scheme` is not set on a Pod."
							required:    false
							type: string: {
								default: "http"
								enum: {
									http:  "Plain HTTP."
									https: "HTTPS."
								}
							}
						}
						extra_field_selector: {
							description: "Additional field selector merged with the built-in filter."
							required:    false
							type: string: {
								default: ""
								examples: ["metadata.namespace=monitoring"]
							}
						}
						kube_config_file: {
							description: """
																			Path to a kubeconfig file.

																			When unset, the source falls back to the local kubeconfig followed by
																			in-cluster service-account credentials.
																			"""
							required: false
							type: string: {}
						}
						label_selector: {
							description: """
																			Kubernetes label selector merged with the built-in `vector.dev/exclude!=true` filter.

																			`extra_label_selector` is accepted as a backwards-compatible alias.
																			"""
							required: false
							type: string: {
								default: ""
								examples: ["tier=frontend"]
							}
						}
						namespaces: {
							description: """
																			Restrict discovery to specific namespaces.

																			When non-empty, only Pods in the listed namespaces are watched. When
																			empty, all namespaces are watched (requires cluster-wide RBAC).
																			"""
							required: false
							type: array: {
								default: []
								items: type: string: examples: ["monitoring"]
							}
						}
						pod_annotation_tags: {
							description: "Allowlist of Pod annotations to add as metric tags."
							required:    false
							type: array: {
								default: []
								items: type: string: examples: ["owner"]
							}
						}
						pod_label_tags: {
							description: """
																			Allowlist of Pod labels to add as metric tags.

																			Each entry is the label key; tags are emitted with the same key on each
																			metric. Default is empty to avoid cardinality blow-ups.
																			"""
							required: false
							type: array: {
								default: []
								items: type: string: examples: ["app", "version"]
							}
						}
						role: {
							description: """
																			Discovery role.

																			Discovery role. Only `pod` is supported in this release.
																			"""
							required: false
							type: string: {
								default: "pod"
								enum: pod: "Discover targets from Pods."
							}
						}
						self_node_name: {
							description: """
																			Override for the node name used by `use_self_node_only`.

																			If unset and `use_self_node_only` is `true`, the `VECTOR_SELF_NODE_NAME`
																			environment variable is read instead.
																			"""
							required: false
							type: string: examples: ["node-01"]
						}
						use_self_node_only: {
							description: """
																			Restrict discovery to Pods on the current node.

																			When enabled, the source filters Pods by `spec.nodeName=<node>` where
																			`<node>` is read from the `VECTOR_SELF_NODE_NAME` environment variable
																			(or the `self_node_name` option). Use this when deploying Vector as a
																			DaemonSet.
																			"""
							required: false
							type: bool: default: false
						}
					}
				}
				static: {
					description: "A static list of scrape URLs."
					required:    true
					type: object: options: urls: {
						description: "URLs to scrape."
						required:    true
						type: array: items: type: string: examples: ["http://localhost:9090/metrics"]
					}
				}
			}
		}
	}
	tls: {
		description: "TLS configuration."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsConfig>"]
	}
}
