package metadata

components: sources: kubernetes_logs_api: {
	title: "Kubernetes Logs API"

	description: """
		Collects Pod logs via the Kubernetes `pods/log` API endpoint
		(`GET /api/v1/namespaces/{namespace}/pods/{pod}/log`), automatically
		enriching data with metadata via the Kubernetes API.

		Unlike the `kubernetes_logs` source, this source does not require
		hostPath mounts or DaemonSet privileges. It runs as a regular Deployment
		and is suited for restricted clusters such as OpenShift or hardened
		GKE/EKS environments where hostPath access is not permitted.
		"""

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		deployment_roles: ["aggregator"]
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		auto_generated:   true
		acknowledgements: false
		collect: {
			checkpoint: enabled: false
			from: {
				service: services.kubernetes

				interface: {
					socket: {
						api: {
							title: "Kubernetes pods/log API"
							url:   urls.kubernetes_api_server
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
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
				The ServiceAccount running Vector must have namespace-scoped RBAC permissions:
				`get` on `pods/log`, and `list`/`watch` on `pods`, `namespaces`, and `nodes`.
				No hostPath or node-level access is required.
				""",
		]
		warnings: [
			"""
				This source streams logs from the Kubernetes API server. At high log
				volumes, this can put additional load on the API server. Consider using
				the `kubernetes_logs` source if you have DaemonSet access available.
				""",
			"""
				Log history is limited to what the Kubernetes API server currently holds
				in memory for a Pod. Logs that have already been evicted from the node
				may not be available.
				""",
		]
		notices: []
	}

	installation: {
		platform_name: "kubernetes"
	}

	configuration: generated.components.sources.kubernetes_logs_api.configuration

	output: logs: line: {
		description: "An individual line from a Pod log, retrieved via the Kubernetes pods/log API."
		fields: {
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
				description: "The raw log line from the Pod."
				required:    true
				type: string: {
					examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
				}
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["kubernetes_logs_api"]
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
				"kubernetes.container_image": "gcr.io/k8s-minikube/storage-provisioner:v3"
				"kubernetes.container_name":  "storage-provisioner"
				"kubernetes.namespace_labels": {
					"kubernetes.io/metadata.name": "kube-system"
				}
				"kubernetes.pod_ip": "192.168.1.1"
				"kubernetes.pod_ips": ["192.168.1.1", "::1"]
				"kubernetes.pod_labels": {
					"addonmanager.kubernetes.io/mode": "Reconcile"
				}
				"kubernetes.pod_name":      "storage-provisioner"
				"kubernetes.pod_namespace": "kube-system"
				"kubernetes.pod_node_name": "minikube"
				"kubernetes.pod_uid":       "93bde4d0-9731-4785-a80e-cd27ba8ad7c2"
				"message":                  "F1015 11:01:46.499073       1 main.go:39] error getting server version: Get \"https://10.96.0.1:443/version?timeout=32s\": dial tcp 10.96.0.1:443: connect: network is unreachable"
				"source_type":              "kubernetes_logs_api"
				"stream":                   "stderr"
				"timestamp":                "2020-10-15T11:01:46.499555308Z"
			}
		},
	]

	how_it_works: {
		enrichment: {
			title: "Enrichment"
			body: """
				Vector enriches log events with Kubernetes metadata by querying the API server
				for Pod, Namespace, and Node objects. A comprehensive list of fields can be
				found in the output docs above.
				"""
		}

		filtering: {
			title: "Filtering"
			body: """
				Vector provides filtering options for Kubernetes log collection:

				* The `extra_field_selector` option specifies additional field selectors to
				  filter Pods with, in addition to the built-in Node filter
				  (`spec.nodeName=<self>`).
				* The `extra_label_selector` option specifies label selectors to filter Pods
				  with, in addition to the built-in `vector.dev/exclude!=true` filter.
				* Pods with the label `vector.dev/exclude: "true"` are skipped automatically.
				"""
		}

		pod_exclusion: {
			title: "Pod exclusion"
			body: """
				By default, Pods that have the label `vector.dev/exclude: "true"` are skipped.
				Additional exclusion rules can be configured via the `extra_label_selector` and
				`extra_field_selector` options.
				"""
		}

		kubernetes_api_communication: {
			title: "Kubernetes API communication"
			body: """
				Vector uses the Kubernetes API for two purposes:

				1. **Log streaming** — Each Pod's log stream is opened via
				   `GET /api/v1/namespaces/{namespace}/pods/{pod}/log?follow=true&timestamps=true`.
				2. **Metadata enrichment** — Pod, Namespace, and Node objects are watched via
				   the standard `list`/`watch` API to annotate events with Kubernetes context.

				When running inside a Kubernetes cluster, Vector uses the in-cluster service
				account credentials automatically.
				"""
		}

		privilege_model: {
			title: "Privilege model"
			body: """
				Unlike the `kubernetes_logs` source, this source does not require any
				node-level access:

				* No `hostPath` volume mounts.
				* No DaemonSet — runs as a standard Deployment.
				* Only namespace-scoped RBAC is needed: `get` on `pods/log`, and
				  `list`/`watch` on `pods`, `namespaces`, and `nodes`.

				This makes it suitable for environments where node-level access is restricted,
				such as OpenShift with restricted SCCs, or multi-tenant Kubernetes clusters.
				"""
		}

		state_management: {
			title: "State management"
			body: """
				This source is stateless. It does not persist a checkpoint on disk.
				On restart, log streaming resumes from the current tail of each Pod's log.
				Events that occurred while Vector was not running may be missed.
				"""
		}

		kubernetes_api_access_control: {
			title: "Kubernetes API access control"
			body: """
				The `kubernetes_logs_api` source requires the following RBAC permissions:

				```yaml
				rules:
				  - apiGroups: [""]
				    resources: ["pods/log"]
				    verbs: ["get"]
				  - apiGroups: [""]
				    resources: ["pods", "namespaces", "nodes"]
				    verbs: ["list", "watch"]
				```

				These can be granted at the namespace level via a `Role` and `RoleBinding`,
				or cluster-wide via a `ClusterRole` and `ClusterRoleBinding`.
				"""
		}
	}
}
