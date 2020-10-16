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
			checkpoint: enabled: false
			from: {
				name:     "Kubernetes"
				thing:    "\(name) nodes"
				url:      urls.kubernetes
				versions: ">= 1.14"

				interface: file_system: {
					directory: _directory
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

		requirements: [
			"""
				[Kubernetes][urls.kubernetes] version `>= 1.14` is required.
				""",
		]
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
	}

	output: logs: line: {
		description: "fill in"
		fields: {}
	}

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
}
