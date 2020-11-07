package metadata

components: sources: kubernetes_logs: {
	_directory: "/var/log"

	title: "Kubernetes Logs"

	classes: {
		commonly_used: true
		delivery:      "best_effort"
		deployment_roles: ["daemon"]
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		collect: {
			checkpoint: enabled: true
			from: {
				name:     "Kubernetes"
				thing:    "\(name) nodes"
				url:      urls.kubernetes
				versions: ">= 1.14"

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
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
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
						}
					}
					container_name: {
						common:      false
						description: "Event field for Container name."
						required:    false
						type: string: {
							default: "kubernetes.container_name"
						}
					}
					pod_labels: {
						common:      false
						description: "Event field for Pod labels."
						required:    false
						type: string: {
							default: "kubernetes.pod_labels"
						}
					}
					pod_name: {
						common:      false
						description: "Event field for Pod name."
						required:    false
						type: string: {
							default: "kubernetes.pod_name"
						}
					}
					pod_namespace: {
						common:      false
						description: "Event field for Pod namespace."
						required:    false
						type: string: {
							default: "kubernetes.pod_namespace"
						}
					}
					pod_node_name: {
						common:      false
						description: "Event field for Pod node_name."
						required:    false
						type: string: {
							default: "kubernetes.pod_node_name"
						}
					}
					pod_uid: {
						common:      false
						description: "Event field for Pod uid."
						required:    false
						type: string: {
							default: "kubernetes.pod_uid"
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
		self_node_name: {
			common:      false
			description: "The name of the Kubernetes `Node` this Vector instance runs at. Configured to use an env var by default, to be evaluated to a value provided by Kubernetes at Pod deploy time."
			required:    false
			type: string: {
				default: "${VECTOR_SELF_NODE_NAME}"
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
				items: type: string: examples: ["**/exclude/**"]
			}
		}
		extra_field_selector: {
			common: false
			description: """
				Specifies the field selector to filter `Pod`s with, to be used in addition to the built-in `Node` filter.
				The name of the Kubernetes `Node` this Vector instance runs at. Configured to use an env var by default, to be evaluated to a value provided by Kubernetes at Pod deploy time.
				"""
			required: false
			type: array: {
				default: []
				items: type: string: examples: ["metadata.name!=pod-name-to-exclude"]
			}
		}
		extra_label_selector: {
			common: false
			description: """
				Specifies the label selector to filter `Pod`s with, to be used in
				addition to the built-in `vector.dev/exclude` filter.
				"""
			required: false
			type: array: {
				default: []
				items: type: string: examples: ["my_custom_label!=my_value"]
			}
		}
	}

	output: logs: line: {
		description: "An individual line from a `Pod` log file."
		fields: {
			file: {
				description: "The absolute path of originating file."
				required:    true
				type: string: examples: ["\(_directory)/pods/pod-namespace_pod-name_pod-uid/container/1.log"]
			}
			"kubernetes.container_image": {
				description: "Container image."
				required:    false
				common:      true
				type: string: {
					examples: ["busybox:1.30"]
					default: null
				}
			}
			"kubernetes.container_name": {
				description: "Container name."
				required:    false
				common:      true
				type: string: {
					examples: ["coredns"]
					default: null
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
				}
			}
			"kubernetes.pod_namespace": {
				description: "Pod namespace."
				required:    false
				common:      true
				type: string: {
					examples: ["kube-system"]
					default: null
				}
			}
			"kubernetes.pod_node_name": {
				description: "Pod node name."
				required:    false
				common:      true
				type: string: {
					examples: ["minikube"]
					default: null
				}
			}
			"kubernetes.pod_uid": {
				description: "Pod uid."
				required:    false
				common:      true
				type: string: {
					examples: ["ba46d8c9-9541-4f6b-bbf9-d23b36f2f136"]
					default: null
				}
			}
			message: {
				description: "The raw line from the Pod log file."
				required:    true
				type: string: examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: examples: ["kubernetes_logs"]
			}
			stream: {
				description: "The name of the stream the log line was sumbitted to."
				required:    true
				type: string: examples: ["stdout", "stderr"]
			}
			timestamp: fields._current_timestamp
		}
	}

	examples: [
		{
			title: "Sample Output"
			configuration: {
				type: "kubernetes_logs"
			}
			input: """
				```text
				F1015 11:01:46.499073       1 main.go:39] error getting server version: Get \"https://10.96.0.1:443/version?timeout=32s\": dial tcp 10.96.0.1:443: connect: network is unreachable
				```
				"""
			output: log: {
				"file":                       "/var/log/pods/kube-system_storage-provisioner_93bde4d0-9731-4785-a80e-cd27ba8ad7c2/storage-provisioner/1.log"
				"kubernetes.container_image": "gcr.io/k8s-minikube/storage-provisioner:v3"
				"kubernetes.container_name":  "storage-provisioner"
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

	how_it_works: {
		connecting_to_kubernetes_api: {
			title: "Connecting To The Kubernetes API server"
			body:  """
					Vector will automatically attempt to connect to the
					[Kubernetes API server](\(urls.kubernetes_api_server)) for
					you. If Vector is running in a Kubernetes cluster then
					Vector will connect to that cluster using the
					[Kubernetes provided access information](\(urls.kubernetes_accessing_api_from_pod)).
					"""
		}
	}

	telemetry: metrics: {
		vector_k8s_docker_format_parse_failures_total: _vector_k8s_docker_format_parse_failures_total
		vector_k8s_event_annotation_failures_total:    _vector_k8s_event_annotation_failures_total
	}
}
