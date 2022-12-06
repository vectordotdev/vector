package metadata

base: components: sources: kubernetes_logs: configuration: {
	auto_partial_merge: {
		description: "Whether or not to automatically merge partial events."
		required:    false
		type: bool: default: true
	}
	data_dir: {
		description: """
			The directory used to persist file checkpoint positions.

			By default, the global `data_dir` option is used. Please make sure the user Vector is running as has write permissions to this directory.
			"""
		required: false
		type: string: syntax: "literal"
	}
	delay_deletion_ms: {
		description: """
			How long to delay removing entries from our map when we receive a deletion
			event from the watched stream.
			"""
		required: false
		type: uint: default: 60000
	}
	exclude_paths_glob_patterns: {
		description: "A list of glob patterns to exclude from reading the files."
		required:    false
		type: array: {
			default: ["**/*.gz", "**/*.tmp"]
			items: type: string: syntax: "literal"
		}
	}
	extra_field_selector: {
		description: "Specifies the field selector to filter `Pod`s with, to be used in addition to the built-in `Node` filter."
		required:    false
		type: string: {
			default: ""
			syntax:  "literal"
		}
	}
	extra_label_selector: {
		description: "Specifies the label selector to filter `Pod`s with, to be used in addition to the built-in `vector.dev/exclude` filter."
		required:    false
		type: string: {
			default: ""
			syntax:  "literal"
		}
	}
	extra_namespace_label_selector: {
		description: "Specifies the label selector to filter `Namespace`s with, to be used in  addition to the built-in `vector.dev/exclude` filter."
		required:    false
		type: string: {
			default: ""
			syntax:  "literal"
		}
	}
	fingerprint_lines: {
		description: "How many first lines in a file are used for fingerprinting."
		required:    false
		type: uint: default: 1
	}
	glob_minimum_cooldown_ms: {
		description: """
			This value specifies not exactly the globbing, but interval
			between the polling the files to watch from the `paths_provider`.
			This is quite efficient, yet might still create some load of the
			file system; in addition, it is currently coupled with chechsum dumping
			in the underlying file server, so setting it too low may introduce
			a significant overhead.
			"""
		required: false
		type: uint: default: 60000
	}
	ingestion_timestamp_field: {
		description: """
			A field to use to set the timestamp when Vector ingested the event.
			This is useful to compute the latency between important event processing
			stages, i.e. the time delta between log line was written and when it was
			processed by the `kubernetes_logs` source.
			"""
		required: false
		type: string: syntax: "literal"
	}
	kube_config_file: {
		description: """
			Optional path to a kubeconfig file readable by Vector. If not set,
			Vector will try to connect to Kubernetes using in-cluster configuration.
			"""
		required: false
		type: string: syntax: "literal"
	}
	max_line_bytes: {
		description: """
			The maximum number of bytes a line can contain before being discarded. This protects
			against malformed lines or tailing incorrect files.
			"""
		required: false
		type: uint: default: 32768
	}
	max_read_bytes: {
		description: """
			Max amount of bytes to read from a single file before switching over
			to the next file.
			This allows distributing the reads more or less evenly across
			the files.
			"""
		required: false
		type: uint: default: 2048
	}
	namespace_annotation_fields: {
		description: "Configuration for how the events are annotated with Namespace metadata."
		required:    false
		type: object: {
			default: namespace_labels: ".kubernetes.namespace_labels"
			options: namespace_labels: {
				description: "Event field for Namespace labels."
				required:    false
				type: string: {
					default: ".kubernetes.namespace_labels"
					syntax:  "literal"
				}
			}
		}
	}
	node_annotation_fields: {
		description: "Configuration for how the events are annotated with Node metadata."
		required:    false
		type: object: {
			default: node_labels: ".kubernetes.node_labels"
			options: node_labels: {
				description: "Event field for Node labels."
				required:    false
				type: string: {
					default: ".kubernetes.node_labels"
					syntax:  "literal"
				}
			}
		}
	}
	pod_annotation_fields: {
		description: "Configuration for how the events are annotated with `Pod` metadata."
		required:    false
		type: object: {
			default: {
				container_id:    ".kubernetes.container_id"
				container_image: ".kubernetes.container_image"
				container_name:  ".kubernetes.container_name"
				pod_annotations: ".kubernetes.pod_annotations"
				pod_ip:          ".kubernetes.pod_ip"
				pod_ips:         ".kubernetes.pod_ips"
				pod_labels:      ".kubernetes.pod_labels"
				pod_name:        ".kubernetes.pod_name"
				pod_namespace:   ".kubernetes.pod_namespace"
				pod_node_name:   ".kubernetes.pod_node_name"
				pod_owner:       ".kubernetes.pod_owner"
				pod_uid:         ".kubernetes.pod_uid"
			}
			options: {
				container_id: {
					description: "Event field for container ID."
					required:    false
					type: string: {
						default: ".kubernetes.container_id"
						syntax:  "literal"
					}
				}
				container_image: {
					description: "Event field for container image."
					required:    false
					type: string: {
						default: ".kubernetes.container_image"
						syntax:  "literal"
					}
				}
				container_name: {
					description: "Event field for container name."
					required:    false
					type: string: {
						default: ".kubernetes.container_name"
						syntax:  "literal"
					}
				}
				pod_annotations: {
					description: "Event field for Pod annotations."
					required:    false
					type: string: {
						default: ".kubernetes.pod_annotations"
						syntax:  "literal"
					}
				}
				pod_ip: {
					description: "Event field for Pod IPv4 address."
					required:    false
					type: string: {
						default: ".kubernetes.pod_ip"
						syntax:  "literal"
					}
				}
				pod_ips: {
					description: "Event field for Pod IPv4 and IPv6 addresses."
					required:    false
					type: string: {
						default: ".kubernetes.pod_ips"
						syntax:  "literal"
					}
				}
				pod_labels: {
					description: "Event field for Pod labels."
					required:    false
					type: string: {
						default: ".kubernetes.pod_labels"
						syntax:  "literal"
					}
				}
				pod_name: {
					description: "Event field for Pod name."
					required:    false
					type: string: {
						default: ".kubernetes.pod_name"
						syntax:  "literal"
					}
				}
				pod_namespace: {
					description: "Event field for Pod namespace."
					required:    false
					type: string: {
						default: ".kubernetes.pod_namespace"
						syntax:  "literal"
					}
				}
				pod_node_name: {
					description: "Event field for Pod node_name."
					required:    false
					type: string: {
						default: ".kubernetes.pod_node_name"
						syntax:  "literal"
					}
				}
				pod_owner: {
					description: "Event field for Pod owner reference."
					required:    false
					type: string: {
						default: ".kubernetes.pod_owner"
						syntax:  "literal"
					}
				}
				pod_uid: {
					description: "Event field for Pod uid."
					required:    false
					type: string: {
						default: ".kubernetes.pod_uid"
						syntax:  "literal"
					}
				}
			}
		}
	}
	self_node_name: {
		description: """
			The `name` of the Kubernetes `Node` that Vector runs at.

			Configured to use an environment var by default, to be evaluated to a value provided by Kubernetes at `Pod` deploy time.
			"""
		required: false
		type: string: {
			default: "${VECTOR_SELF_NODE_NAME}"
			syntax:  "literal"
		}
	}
	timezone: {
		description: "The default time zone for timestamps without an explicit zone."
		required:    false
		type: string: examples: ["local", "America/New_York", "EST5EDT"]
	}
}
