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
		requirements: [
			"""
				[Kubernetes](\(urls.kubernetes)) version `\(services.kubernetes.versions)` is required.
				""",
		]
		warnings: []
		notices: []
	}

	installation: {
		platform_name: "kubernetes"
	}

	configuration: {
		pod_annotation_fields: {
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
					pod_ip: {
						common:      false
						description: "Event field for Pod IPv4 Address."
						required:    false
						type: string: {
							default: "kubernetes.pod_ip"
						}
					}
					pod_ips: {
						common:      false
						description: "Event field for Pod IPv4 and IPv6 Addresses."
						required:    false
						type: string: {
							default: "kubernetes.pod_ips"
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
					pod_annotations: {
						common:      false
						description: "Event field for Pod annotations."
						required:    false
						type: string: {
							default: "kubernetes.pod_annotations"
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
					pod_owner: {
						common:      false
						description: "Event field for Pod owner reference."
						required:    false
						type: string: {
							default: "kubernetes.pod_owner"
						}
					}
				}
			}
		}
		namespace_annotation_fields: {
			common:      false
			description: "Configuration for how the events are annotated with Namespace metadata."
			required:    false
			type: object: {
				examples: []
				options: {
					namespace_labels: {
						common:      false
						description: "Event field for Namespace labels."
						required:    false
						type: string: {
							default: "kubernetes.namespace_labels"
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
		ingestion_timestamp_field: {
			common:      false
			description: "The exact time the event was ingested into Vector."
			required:    false
			type: string: default: null
		}
		kube_config_file: {
			common:      false
			description: "Optional path to a kubeconfig file readable by Vector. If not set, Vector will try to connect to Kubernetes using in-cluster configuration."
			required:    false
			type: string: default: null
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
				default: ["**/*.gz", "**/*.tmp"]
				items: type: string: {
					examples: ["**/exclude/**"]
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
			}
		}
		max_read_bytes: {
			category:    "Reading"
			common:      false
			description: "An approximate limit on the amount of data read from a single pod log file at a given time."
			required:    false
			type: uint: {
				default: 2048
				examples: [2048]
				unit: "bytes"
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
		fingerprint_lines: {
			common: false
			description: """
				The number of lines to read when generating a unique fingerprint of a log file.
				This is helpful when some containers share common first log lines.
				WARNING: If the file has less than this amount of lines then it won't be read at all.
				This is important since container logs are broken up into several files, so the greater
				`lines` value is, the greater the chance of it not reading the last file/logs of
				the container.
				"""
			required: false
			type: uint: {
				default: 1
				unit:    "lines"
			}
		}
		glob_minimum_cooldown_ms: {
			common:      false
			description: "Delay between file discovery calls. This controls the interval at which Vector searches for files within a single pod."
			required:    false
			type: uint: {
				default: 60_000
				unit:    "milliseconds"
			}
		}
		delay_deletion_ms: {
			common: false
			description: """
				Delay between receiving a `DELETE` event and removing any related metadata Vector has stored. This controls how quickly Vector will remove
				metadata for resources that have been removed from Kubernetes, a longer delay will allow Vector to continue processing and enriching logs after the source Pod has been deleted.
				If Vector tries to process logs from a Pod which has already had its metadata removed from the local cache, it will fail to enrich the event with metadata and log a warning.
				"""
			required: false
			type: uint: {
				default: 60_000
				unit:    "milliseconds"
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

				* Built-in [`Pod`](#pod-exclusion) and [`container`](#container-exclusion)
				  exclusion rules.
				* The `exclude_paths_glob_patterns` option allows you to exclude
				  Kubernetes log files by the file name and path.
				* The `extra_field_selector` option specifies the field selector to
				  filter Pods with, to be used in addition to the built-in `Node` filter.
				* The `extra_label_selector` option specifies the label selector to
				  filter `Pod`s with, to be used in addition to the [built-in
				  `vector.dev/exclude` filter](#pod-exclusion).
				"""
		}

		globbing: {
			title: "Globbing"
			body:  """
				By default, the [`kubernetes_logs` source](\(urls.vector_kubernetes_logs_source))
				ignores compressed and temporary files. This behavior can be configured with the
				[`exclude_paths_glob_patterns`](\(urls.vector_kubernetes_logs_source)#configuration) option.

				[Globbing](\(urls.globbing)) is used to continually discover `Pod`s log files
				at a rate defined by the `glob_minimum_cooldown` option. In environments when files are
				rotated rapidly, we recommend lowering the `glob_minimum_cooldown` to catch files
				before they are compressed.
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

					```yaml
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
				with `1.15`.
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
				configuration - Vector will just work.

				Clusters using legacy ABAC scheme are not officially supported
				(although Vector might work if you configure access properly) -
				we encourage switching to RBAC. If you use a custom access control
				scheme - make sure Vector `Pod`/`ServiceAccount` is granted access to
				the `/api/v1/pods` resource.
				"""
		}
	}

	telemetry: metrics: {
		events_in_total:                        components.sources.internal_metrics.output.metrics.events_in_total
		k8s_format_picker_edge_cases_total:     components.sources.internal_metrics.output.metrics.k8s_format_picker_edge_cases_total
		k8s_docker_format_parse_failures_total: components.sources.internal_metrics.output.metrics.k8s_docker_format_parse_failures_total
		k8s_event_annotation_failures_total:    components.sources.internal_metrics.output.metrics.k8s_event_annotation_failures_total
		k8s_reflector_desyncs_total:            components.sources.internal_metrics.output.metrics.k8s_reflector_desyncs_total
		k8s_state_ops_total:                    components.sources.internal_metrics.output.metrics.k8s_state_ops_total
		k8s_stream_chunks_processed_total:      components.sources.internal_metrics.output.metrics.k8s_stream_chunks_processed_total
		k8s_stream_processed_bytes_total:       components.sources.internal_metrics.output.metrics.k8s_stream_processed_bytes_total
		k8s_watch_requests_invoked_total:       components.sources.internal_metrics.output.metrics.k8s_watch_requests_invoked_total
		k8s_watch_requests_failed_total:        components.sources.internal_metrics.output.metrics.k8s_watch_requests_failed_total
		k8s_watch_stream_failed_total:          components.sources.internal_metrics.output.metrics.k8s_watch_stream_failed_total
		k8s_watch_stream_items_obtained_total:  components.sources.internal_metrics.output.metrics.k8s_watch_stream_items_obtained_total
		k8s_watcher_http_error_total:           components.sources.internal_metrics.output.metrics.k8s_watcher_http_error_total
		processed_bytes_total:                  components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:                 components.sources.internal_metrics.output.metrics.processed_events_total
		component_discarded_events_total:       components.sources.internal_metrics.output.metrics.component_discarded_events_total
		component_errors_total:                 components.sources.internal_metrics.output.metrics.component_errors_total
		component_received_bytes_total:         components.sources.internal_metrics.output.metrics.component_received_bytes_total
		component_received_event_bytes_total:   components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		component_received_events_total:        components.sources.internal_metrics.output.metrics.component_received_events_total
	}
}
