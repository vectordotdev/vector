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

	// Note that these sections are also copied over the
	// `installation.platforms.kubernetes.how_it_works` key. Therefore, full
	// URLs should be used in links and language should be used that works in
	// both contexts.
	how_it_works: {
		enrichment: {
			title: "Enrichment"
			body:  """
					Vector will enrich data with Kubernetes context. A comprehensive
					list of fields can be found in the
					[`kubernetes_logs` source output docs](\(urls.vector_kubernetes_logs_source)#output).
					"""
		}

		filtering: {
			title: "Filtering"
			body: """
				Vector provides rich filtering options for Kubernetes log collection:

				* Built-in [`Pod`](#pod-exclusion) and [`container`](#container-exclusion)
				  exclusion rules.
				* The `exclude_paths_glob_patterns` option allows you to exclude
				  Kuberenetes log files by the file name and path.
				* The `extra_field_selector` option specifies the field selector to
				  filter Pods with, to be used in addition to the built-in `Node` filter.
				* The `extra_label_selector` option specifies the label selector to
				  filter `Pod`s with, to be used in addition to the [built-in
				  `vector.dev/exclude` filter](#pod-exclusion).
				"""
		}

		pod_exclusion: {
			title: "Pod exclusion"
			body:  """
					By default, the [`kubernetes_logs` source](\(urls.vector_kubernetes_logs_source))
					will skip logs from the `Pod`s that have a `vector.dev/exclude: "true"` *label*.
					You can configure additional exclusion rules via label or field selectors,
					see [the available options](\(urls.vector_kubernetes_logs_source)#configuration).
					"""
		}

		container_exclusion: {
			title: "Container exclusion"
			body:  """
					The [`kubernetes_logs` source](\(urls.vector_kubernetes_logs_source))
					can skip the logs from the individual `container`s of a particular
					`Pod`. Add an *annotation* `vector.dev/exclude-containers` to the
					`Pod`, and enumerate the `name`s of all the `container`s to exclude in
					the value of the annotation like so:

					```
					vector.dev/exclude-containers: "container1,container2"
					```

					This annotation will make Vector skip logs originating from the
					`container1` and `container2` of the `Pod` marked with the annotation,
					while logs from other `container`s in the `Pod` will still be
					collected.
					"""
		}

		kubernetes_api_communication: {
			title: "Kubernetes API communication"
			body:  """
					Vector communicates with the Kubernetes API to enrich the data it collects with
					Kubernetes context. Therefore, Vector must have access to communicate with the
					[Kubernetes API server](\(urls.kubernetes_api_server)). If Vector is running in
					a Kubernetes cluster then Vector will connect to that cluster using the
					[Kubernetes provided access information](\(urls.kubernetes_accessing_api_from_pod)).

					In addition to access, Vector implements proper desync handling to ensure
					communication is safe and reliable. This ensures that Vector will not overwhelm
					the Kubernetes API or compromise its stability.
					"""
		}

		partial_message_merging: {
			title: "Partial message merging"
			body:  """
					Vector, by default, will merge partial messages that are
					split due to the Docker size limit. For everything else, it
					is recommended to use the [`reduce`
					transform](\(urls.vector_reduce_transform)) which offers
					the ability to handle custom merging of things like
					stacktraces.
					"""
		}

		pod_removal: {
			title: "Pod removal"
			body: """
				To ensure all data is collected, Vector will continue to collect logs from the
				`Pod` for some time after its removal. This ensures that Vector obtains some of
				the most important data, such as crash details.
				"""
		}

		resource_limits: {
			title: "Resource limits"
			body: """
				Vector recommends the following resource limits.
				"""
			sub_sections: [
				{
					title: "Agent resource limits"
					body: """
						If deploy Vector as an agent (collecting data for each of your
						Nodes), then we recommend the following limits:

						```yaml
						resources:
						  requests:
						    memory: "64Mi"
						    cpu: "500m"
						  limits:
						    memory: "1024Mi"
						    cpu: "6000m"
						```

						**As with all Kubernetes resource limit recommendations, use these
						as a reference point and adjust as ncessary. If your configured
						Vector pipeline is complex, you may need more resources. If you
						have a pipeline you may need less.**
						"""
				},
			]
		}

		state_management: {
			title: "State management"
			body:  null
			sub_sections: [
				{
					title: "Agent state management"
					body: """
						For the agent role, Vector stores its state at the host-mapped dir with a static
						path, so if it's redeployed it'll continue from where it was interrupted.
						"""
				},
			]
		}

		testing_and_reliability: {
			title: "Testing & reliability"
			body: """
				Vector is tested extensively against Kubernetes. In addition to Kubernetes
				being Vector's most popular installation method, Vector implements a
				comprehensive end-to-end test suite for all minor Kubernetes versions starting
				with `1.14.
				"""
		}

		kubernetes_api_access_control: {
			title: "Kubernetes API access control"
			body:  """
				Vector requires access to the Kubernetes API.
				Specifically, the [`kubernetes_logs` source](\(urls.vector_kubernetes_logs_source))
				uses the `/api/v1/pods` endpoint to "watch" the pods from
				all namespaces.

				Modern Kubernetes clusters run with RBAC (role-based access control)
				scheme. RBAC-enabled clusters require some configuration to grant Vector
				the authorization to access the Kubernetes API endpoints.	As RBAC is
				currently the standard way of controlling access to the Kubernetes API,
				we ship the necessary configuration out of the box: see `ClusterRole`,
				`ClusterRoleBinding` and a `ServiceAccount` in our `kubectl` YAML
				config, and the `rbac` configuration at the Helm chart.

				If your cluster doesn't use any access control scheme	and doesn't
				restrict access to the Kubernetes API, you don't need to do any extra
				configuration - Vector willjust work.

				Clusters using legacy ABAC scheme are not officially supported
				(although Vector might work if you configure access properly) -
				we encourage switching to RBAC. If you use a custom access control
				scheme - make sure Vector `Pod`/`ServiceAccount` is granted access to
				the `/api/v1/pods` resource.
				"""
		}
	}

	telemetry: metrics: {
		k8s_format_picker_edge_cases_total:     components.sources.internal_metrics.output.metrics.k8s_format_picker_edge_cases_total
		k8s_docker_format_parse_failures_total: components.sources.internal_metrics.output.metrics.k8s_docker_format_parse_failures_total
		k8s_event_annotation_failures_total:    components.sources.internal_metrics.output.metrics.k8s_event_annotation_failures_total
		processed_bytes_total:                  components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:                 components.sources.internal_metrics.output.metrics.processed_events_total
	}
}
