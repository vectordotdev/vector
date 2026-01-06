package metadata

components: sources: kubernetes_events: {
	title: "Kubernetes Events"

	description: """
		Streams [`Event`](https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.29/#event-v1-events-k8s-io) \
		objects from the Kubernetes API so you can monitor changes happening inside your cluster.
		"""

	classes: {
		commonly_used: true
		delivery:      "best_effort"
		deployment_roles: ["deployment"]
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		auto_generated:   true
		acknowledgements: false
		collect: {
			from: {
				service: services.kubernetes
				interface: api: {
					endpoint: "events.k8s.io"
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
				The service account running Vector must be allowed to `list` and `watch` the `events.k8s.io/v1`
				API. Granting the built-in `view` ClusterRole is typically sufficient.
				""",
		]
		warnings: []
		notices: []
	}

	installation: {
		platform_name: "kubernetes"
	}

	configuration: generated.components.sources.kubernetes_events.configuration

	output: logs: record: {
		description: "Represents a Kubernetes [`Event`](https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.29/#event-v1-events-k8s-io) object."
		fields: {
			event: {
				description: "The full Kubernetes event payload."
				required:    true
				type: object: options: {}
			}
			event_uid: {
				description: "Unique identifier of the Kubernetes event."
				required:    true
				type: string: {
					examples: ["6b6890ca-47f8-4b04-ae15-986bfdcae4d5"]
				}
			}
			message: {
				description: "Human-readable description of what happened."
				required:    false
				type: string: {
					examples: ["Created pod: convexio-argo-workflows-server-686559bfd5-wt4n2"]
				}
			}
			namespace: {
				description: "Namespace where the event occurred."
				required:    false
				type: string: {
					examples: ["kube-system"]
				}
			}
			reason: {
				description: "Why the action was taken."
				required:    false
				type: string: {
					examples: ["SuccessfulCreate"]
				}
			}
			reporting_controller: {
				description: "Name of the controller that emitted the event."
				required:    false
				type: string: {
					examples: ["replicaset-controller"]
				}
			}
			reporting_instance: {
				description: "Identifier of the controller instance that emitted the Event."
				required:    false
				type: string: {
					examples: ["kubelet-ip-10-0-0-1"]
				}
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["kubernetes_events"]
				}
			}
			timestamp: fields._current_timestamp
			type: {
				description: "Event type (for example `Normal` or `Warning`)."
				required:    false
				type: string: {
					examples: ["Normal"]
				}
			}
			verb: {
				description: "Derived Vector verb for the Event (`ADDED` or `UPDATED`)."
				required:    true
				type: string: {
					examples: ["ADDED", "UPDATED"]
				}
			}
		}
	}
}
