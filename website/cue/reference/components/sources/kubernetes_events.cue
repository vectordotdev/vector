package metadata

components: sources: kubernetes_events: {
	title: "Kubernetes Events"

	description: """
		Streams [`Event`](https://kubernetes.io/docs/reference/generated/kubernetes-api/v1.29/#event-v1-events-k8s-io)
		objects from the Kubernetes API so you can monitor changes happening inside your cluster.
		"""

	classes: {
		delivery: "best_effort"
		deployment_roles: ["aggregator"]
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
			"""
				When `leader_election.enabled` is `true`, the service account must also be allowed to `get`,
				`create`, and `update` `coordination.k8s.io/v1` Lease objects in the configured lease namespace.
				""",
		]
		warnings: []
		notices: []
	}

	installation: {
		platform_name: "kubernetes"
	}

	configuration: generated.components.sources.kubernetes_events.configuration

	how_it_works: {
		leader_election: {
			title: "Leader election and failover"
			body: """
				With `leader_election.enabled` set to `true`, replicas coordinate through a
				`coordination.k8s.io/v1` Lease so that only one replica streams events at a time.

				The active leader records the last safely handled Kubernetes `resourceVersion` for each
				watch stream as an annotation on the Lease, updated with each renewal. When leadership
				changes hands, the new leader resumes each watch from that API-server checkpoint instead
				of replaying all retained Event objects.

				Delivery into Vector is at-least-once: checkpoints only advance over events that were
				handed to the topology. Duplicates remain possible in the failover window (for example,
				when a partitioned leader keeps sending until its renew deadline expires) or when the
				API server has expired a stored resource version and Vector must perform a fresh list.
				Downstream consumers that need exactly-once semantics can deduplicate on the emitted
				`event_uid` together with the event's `resourceVersion`.
				"""
		}
	}

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
				description: "Identifier of the controller instance that emitted the event."
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
				description: "Derived Vector verb for the event (`ADDED` or `UPDATED`)."
				required:    true
				type: string: {
					examples: ["ADDED", "UPDATED"]
				}
			}
		}
	}
}
