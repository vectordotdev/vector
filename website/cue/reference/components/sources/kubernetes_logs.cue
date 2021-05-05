package metadata

components: sources: kubernetes_logs: {
	_directory: "/var/log"

	title: "Kubernetes Logs"

	description: """
		Collects all log data for Kubernetes Nodes, automatically enriching data
		with Kubernetes metadata via the Kubernetes API.
		"""

	classes: {
		commonly_used: true
		delivery:      "best_effort"
		deployment_roles: ["daemon"]
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		collect: {
			checkpoint: enabled: true
			from: {
				service: services.kubernetes

				interface: {
					file_system: {
						directory: _directory
					}
				}
			}
		}
		multiline: enabled: false
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: "kubernetes"
	}

	configuration: {
		annotation_fields: {
			common:      false
			description: "Configuration for how the events are annotated with Pod metadata."
			required:    false
			type: object: {
				examples: []
				options: {
					container_image: {
						common:      false
						description: "Event field for Container image."
						required:    false
						type: string: {
							default: "kubernetes.container_image"
							syntax:  "literal"
						}
					}
					container_name: {
						common:      false
						description: "Event field for Container name."
						required:    false
						type: string: {
							default: "kubernetes.container_name"
							syntax:  "literal"
						}
					}
					pod_ip: {
						common:      false
						description: "Event field for Pod IPv4 Address."
						required:    false
						type: string: {
							default: "kubernetes.pod_ip"
							syntax:  "literal"
						}
					}
					pod_ips: {
						common:      false
						description: "Event field for Pod IPv4 and IPv6 Addresses."
						required:    false
						type: string: {
							default: "kubernetes.pod_ips"
							syntax:  "literal"
						}
					}
					pod_labels: {
						common:      false
						description: "Event field for Pod labels."
						required:    false
						type: string: {
							default: "kubernetes.pod_labels"
							syntax:  "literal"
						}
					}
					pod_name: {
						common:      false
						description: "Event field for Pod name."
						required:    false
						type: string: {
							default: "kubernetes.pod_name"
							syntax:  "literal"
						}
					}
					pod_namespace: {
						common:      false
						description: "Event field for Pod namespace."
						required:    false
						type: string: {
							default: "kubernetes.pod_namespace"
							syntax:  "literal"
						}
					}
					pod_node_name: {
						common:      false
						description: "Event field for Pod node_name."
						required:    false
						type: string: {
							default: "kubernetes.pod_node_name"
							syntax:  "literal"
						}
					}
					pod_uid: {
						common:      false
						description: "Event field for Pod uid."
						required:    false
						type: string: {
							default: "kubernetes.pod_uid"
							syntax:  "literal"
						}
					}
				}
			}
		}
		auto_partial_merge: {
			common:      false
			description: "Automatically merge partial messages into a single event. Partial here is in respect to messages that were split by the Kubernetes Container Runtime log driver."
			required:    false
			type: bool: default: true
		}
		kube_config_file: {
			common:      false
			description: "Optional path to a kubeconfig file readable by Vector. If not set, Vector will try to connect to Kubernetes using in-cluster configuration."
			required:    false
			type: string: {
				default: null
				syntax:  "literal"
			}
		}
		self_node_name: {
			common:      false
			description: "The name of the Kubernetes `Node` this Vector instance runs at. Configured to use an env var by default, to be evaluated to a value provided by Kubernetes at Pod deploy time."
			required:    false
			type: string: {
				default: "${VECTOR_SELF_NODE_NAME}"
				syntax:  "literal"
			}
		}
		exclude_paths_glob_patterns: {
			common: false
			description: """
				A list of glob patterns to exclude from reading the files.
				"""
			required: false
			type: array: {
				default: []
				items: type: string: {
					examples: ["**/exclude/**"]
					syntax: "literal"
				}
			}
		}
		extra_field_selector: {
			common: false
			description: """
				Specifies the field selector to filter `Pod`s with, to be used in addition to the built-in `Node` filter.
				The name of the Kubernetes `Node` this Vector instance runs at. Configured to use an env var by default, to be evaluated to a value provided by Kubernetes at Pod deploy time.
				"""
			required: false
			type: string: {
				default: ""
				examples: ["metadata.name!=pod-name-to-exclude", "metadata.name!=pod-name-to-exclude,metadata.name=mypod"]
				syntax: "literal"
			}
		}
		extra_label_selector: {
			common: false
			description: """
				Specifies the label selector to filter `Pod`s with, to be used in
				addition to the built-in `vector.dev/exclude` filter.
				"""
			required: false
			type: string: {
				default: ""
				examples: ["my_custom_label!=my_value", "my_custom_label!=my_value,my_other_custom_label=my_value"]
				syntax: "literal"
			}
		}
		max_line_bytes: {
			common:      false
			description: "The maximum number of a bytes a line can contain before being discarded. This protects against malformed lines or tailing incorrect files."
			required:    false
			type: uint: {
				default: 32_768
				unit:    "bytes"
			}
		}
		timezone: configuration._timezone
	}

	output: logs: line: {
		description: "An individual line from a `Pod` log file."
		fields: {
			file: {
				description: "The absolute path of originating file."
				required:    true
				type: string: {
					examples: ["\(_directory)/pods/pod-namespace_pod-name_pod-uid/container/1.log"]
					syntax: "literal"
				}
			}
			"kubernetes.container_image": {
				description: "Container image."
				required:    false
				common:      true
				type: string: {
					examples: ["busybox:1.30"]
					default: null
					syntax:  "literal"
				}
			}
			"kubernetes.container_name": {
				description: "Container name."
				required:    false
				common:      true
				type: string: {
					examples: ["coredns"]
					default: null
					syntax:  "literal"
				}
			}
			"kubernetes.pod_ip": {
				description: "Pod IPv4 address."
				required:    false
				common:      true
				type: string: {
					examples: ["192.168.1.1"]
					default: null
					syntax:  "literal"
				}
			}
			"kubernetes.pod_ips": {
				description: "Pod IPv4 and IPv6 addresses."
				required:    false
				common:      true
				type: string: {
					examples: ["192.168.1.1", "::1"]
					default: null
					syntax:  "literal"
				}
			}
			"kubernetes.pod_labels": {
				description: "Pod labels name."
				required:    false
				common:      true
				type: object: {
					examples: [{"mylabel": "myvalue"}]
					options: {}
				}
			}
			"kubernetes.pod_name": {
				description: "Pod name."
				required:    false
				common:      true
				type: string: {
					examples: ["coredns-qwertyuiop-qwert"]
					default: null
					syntax:  "literal"
				}
			}
			"kubernetes.pod_namespace": {
				description: "Pod namespace."
				required:    false
				common:      true
				type: string: {
					examples: ["kube-system"]
					default: null
					syntax:  "literal"
				}
			}
			"kubernetes.pod_node_name": {
				description: "Pod node name."
				required:    false
				common:      true
				type: string: {
					examples: ["minikube"]
					default: null
					syntax:  "literal"
				}
			}
			"kubernetes.pod_uid": {
				description: "Pod uid."
				required:    false
				common:      true
				type: string: {
					examples: ["ba46d8c9-9541-4f6b-bbf9-d23b36f2f136"]
					default: null
					syntax:  "literal"
				}
			}
			message: {
				description: "The raw line from the Pod log file."
				required:    true
				type: string: {
					examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
					syntax: "literal"
				}
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["kubernetes_logs"]
					syntax: "literal"
				}
			}
			stream: {
				description: "The name of the stream the log line was sumbitted to."
				required:    true
				type: string: {
					examples: ["stdout", "stderr"]
					syntax: "literal"
				}
			}
			timestamp: fields._current_timestamp
		}
	}

	examples: [
		{
			title: "Sample Output"
			configuration: {}
			input: """
				```text
				F1015 11:01:46.499073       1 main.go:39] error getting server version: Get \"https://10.96.0.1:443/version?timeout=32s\": dial tcp 10.96.0.1:443: connect: network is unreachable
				```
				"""
			output: log: {
				"file":                       "/var/log/pods/kube-system_storage-provisioner_93bde4d0-9731-4785-a80e-cd27ba8ad7c2/storage-provisioner/1.log"
				"kubernetes.container_image": "gcr.io/k8s-minikube/storage-provisioner:v3"
				"kubernetes.container_name":  "storage-provisioner"
				"kubernetes.pod_ip":          "192.168.1.1"
				"kubernetes.pod_ips": ["192.168.1.1", "::1"]
				"kubernetes.pod_labels": {
					"addonmanager.kubernetes.io/mode": "Reconcile"
					"gcp-auth-skip-secret":            "true"
					"integration-test":                "storage-provisioner"
				}
				"kubernetes.pod_name":      "storage-provisioner"
				"kubernetes.pod_namespace": "kube-system"
				"kubernetes.pod_node_name": "minikube"
				"kubernetes.pod_uid":       "93bde4d0-9731-4785-a80e-cd27ba8ad7c2"
				"message":                  "F1015 11:01:46.499073       1 main.go:39] error getting server version: Get \"https://10.96.0.1:443/version?timeout=32s\": dial tcp 10.96.0.1:443: connect: network is unreachable"
				"source_type":              "kubernetes_logs"
				"stream":                   "stderr"
				"timestamp":                "2020-10-15T11:01:46.499555308Z"
			}
		},
	]

	telemetry: metrics: {
		events_in_total:                        components.sources.internal_metrics.output.metrics.events_in_total
		k8s_format_picker_edge_cases_total:     components.sources.internal_metrics.output.metrics.k8s_format_picker_edge_cases_total
		k8s_docker_format_parse_failures_total: components.sources.internal_metrics.output.metrics.k8s_docker_format_parse_failures_total
		k8s_event_annotation_failures_total:    components.sources.internal_metrics.output.metrics.k8s_event_annotation_failures_total
		processed_bytes_total:                  components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:                 components.sources.internal_metrics.output.metrics.processed_events_total
	}
}
