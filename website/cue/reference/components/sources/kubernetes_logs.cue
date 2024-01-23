package metadata

components: sources: kubernetes_logs: {
	_directory: "/var/log"

	title: "Kubernetes Logs"

	description: """
		Collects Pod logs from Kubernetes Nodes, automatically enriching data
		with metadata via the Kubernetes API.
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
		auto_generated:   true
		acknowledgements: false
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
		requirements: [
			"""
				[Kubernetes](\(urls.kubernetes)) version `\(services.kubernetes.versions)` is required.
				""",
			"""
				This source requires read access to the `/var/log/pods` directory. When run in a
				Kubernetes cluster this can be provided with a [hostPath](\(urls.kubernetes_host_path)) volume.
				""",
		]
		warnings: ["""
				This source is only tested on Linux. Your mileage may vary for clusters on Windows.
			"""]
		notices: []
	}

	installation: {
		platform_name: "kubernetes"
	}

	configuration: base.components.sources.kubernetes_logs.configuration

	output: logs: line: {
		description: "An individual line from a `Pod` log file."
		fields: {
			file: {
				description: "The absolute path of originating file."
				required:    true
				type: string: {
					examples: ["\(_directory)/pods/pod-namespace_pod-name_pod-uid/container/1.log"]
				}
			}
			"kubernetes.container_id": {
				description: "Container id."
				required:    false
				common:      true
				type: string: {
					default: null
					examples: ["docker://f24c81dcd531c5d353751c77fe0556a4f602f7714c72b9a58f9b26c0628f1fa6"]
				}
			}
			"kubernetes.container_image": {
				description: "Container image."
				required:    false
				common:      true
				type: string: {
					default: null
					examples: ["busybox:1.30"]
				}
			}
			"kubernetes.container_image_id": {
				description: "Container image ID."
				required:    false
				common:      true
				type: string: {
					default: null
					examples: ["busybox@sha256:1e7b63c09af457b93c17d25ef4e6aee96b5bb95f087840cffd7c4bb2fe8ae5c6"]
				}
			}
			"kubernetes.container_name": {
				description: "Container name."
				required:    false
				common:      true
				type: string: {
					default: null
					examples: ["coredns"]
				}
			}
			"kubernetes.namespace_labels": {
				description: "Set of labels attached to the Namespace."
				required:    false
				common:      true
				type: object: {
					examples: [{"mylabel": "myvalue"}]
					options: {}
				}
			}
			"kubernetes.pod_ip": {
				description: "Pod IPv4 address."
				required:    false
				common:      true
				type: string: {
					default: null
					examples: ["192.168.1.1"]
				}
			}
			"kubernetes.pod_ips": {
				description: "Pod IPv4 and IPv6 addresses."
				required:    false
				common:      true
				type: string: {
					default: null
					examples: ["192.168.1.1", "::1"]
				}
			}
			"kubernetes.pod_labels": {
				description: "Set of labels attached to the Pod."
				required:    false
				common:      true
				type: object: {
					examples: [{"mylabel": "myvalue"}]
					options: {}
				}
			}
			"kubernetes.pod_annotations": {
				description: "Set of annotations attached to the Pod."
				required:    false
				common:      true
				type: object: {
					examples: [{"myannotation": "myvalue"}]
					options: {}
				}
			}
			"kubernetes.pod_name": {
				description: "Pod name."
				required:    false
				common:      true
				type: string: {
					default: null
					examples: ["coredns-qwertyuiop-qwert"]
				}
			}
			"kubernetes.pod_namespace": {
				description: "Pod namespace."
				required:    false
				common:      true
				type: string: {
					default: null
					examples: ["kube-system"]
				}
			}
			"kubernetes.pod_node_name": {
				description: "Pod node name."
				required:    false
				common:      true
				type: string: {
					default: null
					examples: ["minikube"]
				}
			}
			"kubernetes.pod_owner": {
				description: "Pod owner."
				required:    false
				common:      true
				type: string: {
					default: null
					examples: ["ReplicaSet/coredns-565d847f94"]
				}
			}
			"kubernetes.pod_uid": {
				description: "Pod uid."
				required:    false
				common:      true
				type: string: {
					default: null
					examples: ["ba46d8c9-9541-4f6b-bbf9-d23b36f2f136"]
				}
			}
			message: {
				description: "The raw line from the Pod log file."
				required:    true
				type: string: {
					examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
				}
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["kubernetes_logs"]
				}
			}
			stream: {
				description: "The name of the stream the log line was submitted to."
				required:    true
				type: string: {
					examples: ["stdout", "stderr"]
				}
			}
			timestamp: fields._current_timestamp & {
				description: "The exact time the event was processed by Kubernetes."
			}
		}
	}

	examples: [
		{
			title: "Sample Output"
			configuration: {}
			input: """
				F1015 11:01:46.499073       1 main.go:39] error getting server version: Get \"https://10.96.0.1:443/version?timeout=32s\": dial tcp 10.96.0.1:443: connect: network is unreachable
				"""
			output: log: {
				"file":                       "/var/log/pods/kube-system_storage-provisioner_93bde4d0-9731-4785-a80e-cd27ba8ad7c2/storage-provisioner/1.log"
				"kubernetes.container_image": "gcr.io/k8s-minikube/storage-provisioner:v3"
				"kubernetes.container_name":  "storage-provisioner"
				"kubernetes.namespace_labels": {
					"kubernetes.io/metadata.name": "kube-system"
				}
				"kubernetes.pod_ip": "192.168.1.1"
				"kubernetes.pod_ips": ["192.168.1.1", "::1"]
				"kubernetes.pod_labels": {
					"addonmanager.kubernetes.io/mode": "Reconcile"
					"gcp-auth-skip-secret":            "true"
					"integration-test":                "storage-provisioner"
				}
				"kubernetes.pod_annotations": {
					"prometheus.io/scrape": "false"
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
	// `administration.platforms.kubernetes.how_it_works` key. Therefore, full
	// URLs should be used in links and language should be used that works in
	// both contexts.
	how_it_works: {
		enrichment: {
			title: "Enrichment"
			body:  """
					Vector will enrich data with Kubernetes context. A comprehensive
					list of fields can be found in the
					[`kubernetes_logs` source output docs](\(urls.vector_kubernetes_logs_source)#output-data).
					"""
		}

		filtering: {
			title: "Filtering"
			body: """
				Vector provides rich filtering options for Kubernetes log collection:

				* Built-in [Pod](#pod-exclusion) and [Container](#container-exclusion)
				  exclusion rules.
				* The `exclude_paths_glob_patterns` option allows you to exclude
				  Kubernetes log files by the file name and path.
				* The `extra_field_selector` option specifies the field selector to
				  filter Pods with, to be used in addition to the built-in Node filter.
				* The `extra_label_selector` option specifies the label selector to
				  filter Pods with, to be used in addition to the [built-in
				  `vector.dev/exclude` filter](#pod-exclusion).
				"""
		}

		globbing: {
			title: "Globbing"
			body:  """
				By default, the [`kubernetes_logs` source](\(urls.vector_kubernetes_logs_source))
				ignores compressed and temporary files. This behavior can be configured with the
				[`exclude_paths_glob_patterns`](\(urls.vector_kubernetes_logs_source)#configuration) option.

				[Globbing](\(urls.globbing)) is used to continually discover Pods' log files
				at a rate defined by the `glob_minimum_cooldown` option. In environments when files are
				rotated rapidly, we recommend lowering the `glob_minimum_cooldown` to catch files
				before they are compressed.
				"""
		}

		namespace_exclusion: {
			title: "Namespace exclusion"
			body:  """
					By default, the [`kubernetes_logs` source](\(urls.vector_kubernetes_logs_source))
					will skip logs from the Namespaces that have a `vector.dev/exclude: "true"` **label**.
					You can configure additional exclusion rules via label selectors,
					see [the available options](\(urls.vector_kubernetes_logs_source)#configuration).
					"""
		}

		pod_exclusion: {
			title: "Pod exclusion"
			body:  """
					By default, the [`kubernetes_logs` source](\(urls.vector_kubernetes_logs_source))
					will skip logs from the Pods that have a `vector.dev/exclude: "true"` **label**.
					You can configure additional exclusion rules via label or field selectors,
					see [the available options](\(urls.vector_kubernetes_logs_source)#configuration).
					"""
		}

		container_exclusion: {
			title: "Container exclusion"
			body:  """
					The [`kubernetes_logs` source](\(urls.vector_kubernetes_logs_source))
					can skip the logs from the individual Containers of a particular
					Pod. Add an **annotation** `vector.dev/exclude-containers` to the
					Pod, and enumerate the names of all the Containers to exclude in
					the value of the annotation like so:

					```yaml
					vector.dev/exclude-containers: "container1,container2"
					```

					This annotation will make Vector skip logs originating from the
					_container1_ and _container2_ of the Pod marked with the annotation,
					while logs from other Containers in the Pod will still be collected.
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
				Pod for some time after its removal. This ensures that Vector obtains some of
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

						As with all Kubernetes resource limit recommendations, **use these
						as a reference point and adjust as necessary**. If your configured
						Vector pipeline is complex, you may need more resources; if you
						have a more straightforward pipeline, you may need less.
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
				with `1.19`.
				"""
		}

		kubernetes_api_access_control: {
			title: "Kubernetes API access control"
			body:  """
				Vector requires access to the Kubernetes API.
				Specifically, the [`kubernetes_logs` source](\(urls.vector_kubernetes_logs_source))
				uses the `/api/v1/pods`, `/api/v1/namespaces`, and `/api/v1/nodes` endpoints
				to `list` and `watch` resources we use to enrich events with additional metadata.

				Modern Kubernetes clusters run with RBAC (role-based access control)
				scheme. RBAC-enabled clusters require some configuration to grant Vector
				the authorization to access the Kubernetes API endpoints.	As RBAC is
				currently the standard way of controlling access to the Kubernetes API,
				we ship the necessary configuration out of the box: see the [ClusterRole, ClusterRoleBinding][rbac],
				and [ServiceAccount][serviceaccount] in our Kubectl YAML
				config, and the [`rbac.yaml`][rbac_helm] template configuration of the Helm chart.

				If your cluster doesn't use any access control scheme	and doesn't
				restrict access to the Kubernetes API, you don't need to do any extra
				configuration - Vector will just work.

				Clusters using legacy ABAC scheme are not officially supported
				(although Vector might work if you configure access properly) -
				we encourage switching to RBAC. If you use a custom access control
				scheme - make sure Vector's Pod/ServiceAccount is granted `list` and `watch` access
				to the `/api/v1/pods`, `/api/v1/namespaces`, and `/api/v1/nodes` resources.

				[serviceaccount]: https://github.com/vectordotdev/vector/blob/master/distribution/kubernetes/vector-agent/serviceaccount.yaml
				[rbac]: https://github.com/vectordotdev/vector/blob/master/distribution/kubernetes/vector-agent/rbac.yaml
				[rbac_helm]: https://github.com/vectordotdev/helm-charts/blob/develop/charts/vector/templates/rbac.yaml
				"""
		}
	}

	telemetry: metrics: {
		k8s_format_picker_edge_cases_total:     components.sources.internal_metrics.output.metrics.k8s_format_picker_edge_cases_total
		k8s_docker_format_parse_failures_total: components.sources.internal_metrics.output.metrics.k8s_docker_format_parse_failures_total
		k8s_reflector_desyncs_total:            components.sources.internal_metrics.output.metrics.k8s_reflector_desyncs_total
		k8s_state_ops_total:                    components.sources.internal_metrics.output.metrics.k8s_state_ops_total
		k8s_stream_chunks_processed_total:      components.sources.internal_metrics.output.metrics.k8s_stream_chunks_processed_total
		k8s_stream_processed_bytes_total:       components.sources.internal_metrics.output.metrics.k8s_stream_processed_bytes_total
		k8s_watch_requests_invoked_total:       components.sources.internal_metrics.output.metrics.k8s_watch_requests_invoked_total
		k8s_watch_requests_failed_total:        components.sources.internal_metrics.output.metrics.k8s_watch_requests_failed_total
		k8s_watch_stream_failed_total:          components.sources.internal_metrics.output.metrics.k8s_watch_stream_failed_total
		k8s_watch_stream_items_obtained_total:  components.sources.internal_metrics.output.metrics.k8s_watch_stream_items_obtained_total
		k8s_watcher_http_error_total:           components.sources.internal_metrics.output.metrics.k8s_watcher_http_error_total
	}
}
