package metadata

base: components: sources: kubernetes_logs: configuration: {
	auto_partial_merge: {
		description: """
			Whether or not to automatically merge partial events.

			Partial events are messages that were split by the Kubernetes Container Runtime
			log driver.
			"""
		required: false
		type: bool: default: true
	}
	data_dir: {
		description: """
			The directory used to persist file checkpoint positions.

			By default, the [global `data_dir` option][global_data_dir] is used.
			Make sure the running user has write permissions to this directory.

			If this directory is specified, then Vector will attempt to create it.

			[global_data_dir]: https://vector.dev/docs/reference/configuration/global-options/#data_dir
			"""
		required: false
		type: string: examples: ["/var/local/lib/vector/"]
	}
	delay_deletion_ms: {
		description: """
			How long to delay removing metadata entries from the cache when a pod deletion event
			event is received from the watch stream.

			A longer delay allows for continued enrichment of logs after the originating Pod is
			removed. If relevant metadata has been removed, the log is forwarded un-enriched and a
			warning is emitted.
			"""
		required: false
		type: uint: {
			default: 60000
			unit:    "milliseconds"
		}
	}
	exclude_paths_glob_patterns: {
		description: "A list of glob patterns to exclude from reading the files."
		required:    false
		type: array: {
			default: ["**/*.gz", "**/*.tmp"]
			items: type: string: examples: ["**/exclude/**"]
		}
	}
	extra_field_selector: {
		description: """
			Specifies the [field selector][field_selector] to filter Pods with, to be used in addition
			to the built-in [Node][node] filter.

			The built-in Node filter uses `self_node_name` to only watch Pods located on the same Node.

			[field_selector]: https://kubernetes.io/docs/concepts/overview/working-with-objects/field-selectors/
			[node]: https://kubernetes.io/docs/concepts/architecture/nodes/
			"""
		required: false
		type: string: {
			default: ""
			examples: ["metadata.name!=pod-name-to-exclude", "metadata.name!=pod-name-to-exclude,metadata.name=mypod"]
		}
	}
	extra_label_selector: {
		description: """
			Specifies the [label selector][label_selector] to filter [Pods][pods] with, to be used in
			addition to the built-in [exclude][exclude] filter.

			[label_selector]: https://kubernetes.io/docs/concepts/overview/working-with-objects/labels/#label-selectors
			[pods]: https://kubernetes.io/docs/concepts/workloads/pods/
			[exclude]: https://vector.dev/docs/reference/configuration/sources/kubernetes_logs/#pod-exclusion
			"""
		required: false
		type: string: {
			default: ""
			examples: ["my_custom_label!=my_value", "my_custom_label!=my_value,my_other_custom_label=my_value"]
		}
	}
	extra_namespace_label_selector: {
		description: """
			Specifies the [label selector][label_selector] to filter [Namespaces][namespaces] with, to
			be used in addition to the built-in [exclude][exclude] filter.

			[label_selector]: https://kubernetes.io/docs/concepts/overview/working-with-objects/labels/#label-selectors
			[namespaces]: https://kubernetes.io/docs/concepts/overview/working-with-objects/namespaces/
			[exclude]: https://vector.dev/docs/reference/configuration/sources/kubernetes_logs/#namespace-exclusion
			"""
		required: false
		type: string: {
			default: ""
			examples: ["my_custom_label!=my_value", "my_custom_label!=my_value,my_other_custom_label=my_value"]
		}
	}
	fingerprint_lines: {
		description: """
			The number of lines to read for generating the checksum.

			If your files share a common header that is not always a fixed size,

			If the file has less than this amount of lines, it won’t be read at all.
			"""
		required: false
		type: uint: {
			default: 1
			unit:    "lines"
		}
	}
	glob_minimum_cooldown_ms: {
		description: """
			The interval at which the file system is polled to identify new files to read from.

			This is quite efficient, yet might still create some load on the
			file system; in addition, it is currently coupled with checksum dumping
			in the underlying file server, so setting it too low may introduce
			a significant overhead.
			"""
		required: false
		type: uint: {
			default: 60000
			unit:    "milliseconds"
		}
	}
	ignore_older_secs: {
		description: "Ignore files with a data modification date older than the specified number of seconds."
		required:    false
		type: uint: {
			examples: [
				600,
			]
			unit: "seconds"
		}
	}
	ingestion_timestamp_field: {
		description: """
			Overrides the name of the log field used to add the ingestion timestamp to each event.

			This is useful to compute the latency between important event processing
			stages. For example, the time delta between when a log line was written and when it was
			processed by the `kubernetes_logs` source.
			"""
		required: false
		type: string: examples: [".ingest_timestamp", "ingest_ts"]
	}
	internal_metrics: {
		description: "Configuration of internal metrics for file-based components."
		required:    false
		type: object: options: include_file_tag: {
			description: """
				Whether or not to include the "file" tag on the component's corresponding internal metrics.

				This is useful for distinguishing between different files while monitoring. However, the tag's
				cardinality is unbounded.
				"""
			required: false
			type: bool: default: false
		}
	}
	kube_config_file: {
		description: """
			Optional path to a readable [kubeconfig][kubeconfig] file.

			If not set, a connection to Kubernetes is made using the in-cluster configuration.

			[kubeconfig]: https://kubernetes.io/docs/concepts/configuration/organize-cluster-access-kubeconfig/
			"""
		required: false
		type: string: examples: ["/path/to/.kube/config"]
	}
	max_line_bytes: {
		description: """
			The maximum number of bytes a line can contain before being discarded.

			This protects against malformed lines or tailing incorrect files.
			"""
		required: false
		type: uint: {
			default: 32768
			unit:    "bytes"
		}
	}
	max_read_bytes: {
		description: """
			Max amount of bytes to read from a single file before switching over to the next file.
			**Note:** This does not apply when `oldest_first` is `true`.

			This allows distributing the reads more or less evenly across
			the files.
			"""
		required: false
		type: uint: {
			default: 2048
			unit:    "bytes"
		}
	}
	namespace_annotation_fields: {
		description: "Configuration for how the events are enriched with Namespace metadata."
		required:    false
		type: object: options: namespace_labels: {
			description: """
				Event field for the Namespace's labels.

				Set to `""` to suppress this key.
				"""
			required: false
			type: string: {
				default: ".kubernetes.namespace_labels"
				examples: [".k8s.ns_labels", "k8s.ns_labels", ""]
			}
		}
	}
	node_annotation_fields: {
		description: "Configuration for how the events are enriched with Node metadata."
		required:    false
		type: object: options: node_labels: {
			description: """
				Event field for the Node's labels.

				Set to `""` to suppress this key.
				"""
			required: false
			type: string: {
				default: ".kubernetes.node_labels"
				examples: [".k8s.node_labels", "k8s.node_labels", ""]
			}
		}
	}
	oldest_first: {
		description: "Instead of balancing read capacity fairly across all watched files, prioritize draining the oldest files before moving on to read data from more recent files."
		required:    false
		type: bool: default: true
	}
	pod_annotation_fields: {
		description: "Configuration for how the events are enriched with Pod metadata."
		required:    false
		type: object: options: {
			container_id: {
				description: """
					Event field for the Container's ID.

					Set to `""` to suppress this key.
					"""
				required: false
				type: string: {
					default: ".kubernetes.container_id"
					examples: [".k8s.container_id", "k8s.container_id", ""]
				}
			}
			container_image: {
				description: """
					Event field for the Container's image.

					Set to `""` to suppress this key.
					"""
				required: false
				type: string: {
					default: ".kubernetes.container_image"
					examples: [".k8s.container_image", "k8s.container_image", ""]
				}
			}
			container_image_id: {
				description: """
					Event field for the Container's image ID.

					Set to `""` to suppress this key.
					"""
				required: false
				type: string: {
					default: ".kubernetes.container_image_id"
					examples: [".k8s.container_image_id", "k8s.container_image_id", ""]
				}
			}
			container_name: {
				description: """
					Event field for the Container's name.

					Set to `""` to suppress this key.
					"""
				required: false
				type: string: {
					default: ".kubernetes.container_name"
					examples: [".k8s.container_name", "k8s.container_name", ""]
				}
			}
			pod_annotations: {
				description: """
					Event field for the Pod's annotations.

					Set to `""` to suppress this key.
					"""
				required: false
				type: string: {
					default: ".kubernetes.pod_annotations"
					examples: [".k8s.pod_annotations", "k8s.pod_annotations", ""]
				}
			}
			pod_ip: {
				description: """
					Event field for the Pod's IPv4 address.

					Set to `""` to suppress this key.
					"""
				required: false
				type: string: {
					default: ".kubernetes.pod_ip"
					examples: [".k8s.pod_ip", "k8s.pod_ip", ""]
				}
			}
			pod_ips: {
				description: """
					Event field for the Pod's IPv4 and IPv6 addresses.

					Set to `""` to suppress this key.
					"""
				required: false
				type: string: {
					default: ".kubernetes.pod_ips"
					examples: [".k8s.pod_ips", "k8s.pod_ips", ""]
				}
			}
			pod_labels: {
				description: """
					Event field for the `Pod`'s labels.

					Set to `""` to suppress this key.
					"""
				required: false
				type: string: {
					default: ".kubernetes.pod_labels"
					examples: [".k8s.pod_labels", "k8s.pod_labels", ""]
				}
			}
			pod_name: {
				description: """
					Event field for the Pod's name.

					Set to `""` to suppress this key.
					"""
				required: false
				type: string: {
					default: ".kubernetes.pod_name"
					examples: [".k8s.pod_name", "k8s.pod_name", ""]
				}
			}
			pod_namespace: {
				description: """
					Event field for the Pod's namespace.

					Set to `""` to suppress this key.
					"""
				required: false
				type: string: {
					default: ".kubernetes.pod_namespace"
					examples: [".k8s.pod_ns", "k8s.pod_ns", ""]
				}
			}
			pod_node_name: {
				description: """
					Event field for the Pod's node_name.

					Set to `""` to suppress this key.
					"""
				required: false
				type: string: {
					default: ".kubernetes.pod_node_name"
					examples: [".k8s.pod_host", "k8s.pod_host", ""]
				}
			}
			pod_owner: {
				description: """
					Event field for the Pod's owner reference.

					Set to `""` to suppress this key.
					"""
				required: false
				type: string: {
					default: ".kubernetes.pod_owner"
					examples: [".k8s.pod_owner", "k8s.pod_owner", ""]
				}
			}
			pod_uid: {
				description: """
					Event field for the Pod's UID.

					Set to `""` to suppress this key.
					"""
				required: false
				type: string: {
					default: ".kubernetes.pod_uid"
					examples: [".k8s.pod_uid", "k8s.pod_uid", ""]
				}
			}
		}
	}
	read_from: {
		description: "File position to use when reading a new file."
		required:    false
		type: string: {
			default: "beginning"
			enum: {
				beginning: "Read from the beginning of the file."
				end:       "Start reading from the current end of the file."
			}
		}
	}
	self_node_name: {
		description: """
			The name of the Kubernetes [Node][node] that is running.

			Configured to use an environment variable by default, to be evaluated to a value provided by
			Kubernetes at Pod creation.

			[node]: https://kubernetes.io/docs/concepts/architecture/nodes/
			"""
		required: false
		type: string: default: "${VECTOR_SELF_NODE_NAME}"
	}
	timezone: {
		description: "The default time zone for timestamps without an explicit zone."
		required:    false
		type: string: examples: ["local", "America/New_York", "EST5EDT"]
	}
	use_apiserver_cache: {
		description: "Determines if requests to the kube-apiserver can be served by a cache."
		required:    false
		type: bool: default: false
	}
}
