package metadata

generated: components: sources: kubernetes_events: configuration: {
	dedupe_retention_seconds: {
		description: """
			Retention window for deduplication state.

			Must be greater than zero. This should be at least as large as `max_event_age_seconds`;
			otherwise a watch restart can re-emit events that are older than the retention window but
			still within the maximum age.
			"""
		required: false
		type: uint: {
			default: 3600
			unit:    "seconds"
		}
	}
	field_selector: {
		description: "Field selector applied to the events list/watch request."
		required:    false
		type: string: examples: ["regarding.kind=Pod"]
	}
	include_involved_object_kinds: {
		description: "Restricts the source to the specified involved object kinds. Empty means all kinds."
		required:    false
		type: array: {
			default: []
			items: type: string: examples: ["Pod"]
		}
	}
	include_previous_event: {
		description: "When enabled, the previous version of the event is included in the emitted payload on updates."
		required:    false
		type: bool: default: false
	}
	include_reasons: {
		description: "Restricts the source to the specified reasons. Empty means all reasons."
		required:    false
		type: array: {
			default: []
			items: type: string: examples: ["FailedScheduling"]
		}
	}
	include_types: {
		description: "Restricts the source to the specified event types (for example, `Warning`). Empty means all types."
		required:    false
		type: array: {
			default: []
			items: type: string: examples: ["Warning"]
		}
	}
	kube_config_file: {
		description: "Path to a kubeconfig file. If omitted, in-cluster configuration or the local kubeconfig is used."
		required:    false
		type: string: examples: ["/path/to/kubeconfig"]
	}
	label_selector: {
		description: "Label selector applied to the events list/watch request."
		required:    false
		type: string: examples: ["type=Warning"]
	}
	leader_election: {
		description: """
			Lease-based leader election settings for running multiple replicas safely.

			The elected leader stores the last safely handled Kubernetes `resourceVersion` for each
			watch stream as an annotation on the Lease object. When another replica takes over, it
			resumes each watch from that checkpoint. Duplicates can still occur during the failover
			window, so downstream consumers that require exactly-once behavior should deduplicate on
			the emitted `event_uid` and the event's `resourceVersion`.
			"""
		required: false
		type: object: options: {
			enabled: {
				description: "Enables Lease-based leader election."
				required:    false
				type: bool: default: false
			}
			identity_env_var: {
				description: """
					Environment variable containing this replica's leader election identity.

					If this variable is not set, Vector falls back to `HOSTNAME`.
					"""
				required: false
				type: string: {
					default: "VECTOR_SELF_POD_NAME"
					examples: ["VECTOR_SELF_POD_NAME"]
				}
			}
			lease_duration_seconds: {
				description: "Lease duration."
				required:    false
				type: uint: {
					default: 15
					unit:    "seconds"
				}
			}
			lease_name: {
				description: "Name of the Kubernetes Lease object used for coordination."
				required:    false
				type: string: {
					default: "vector-kubernetes-events"
					examples: ["vector-kubernetes-events"]
				}
			}
			lease_namespace: {
				description: """
					Namespace containing the Kubernetes Lease object.

					If omitted, Vector uses `VECTOR_SELF_POD_NAMESPACE`, then the in-cluster service account
					namespace file, then `default`.
					"""
				required: false
				type: string: examples: ["observability"]
			}
			renew_deadline_seconds: {
				description: "Maximum time this replica will continue as leader without a successful renewal."
				required:    false
				type: uint: {
					default: 10
					unit:    "seconds"
				}
			}
			retry_period_seconds: {
				description: "Time between leader election acquire and renew attempts."
				required:    false
				type: uint: {
					default: 2
					unit:    "seconds"
				}
			}
		}
	}
	max_event_age_seconds: {
		description: "Maximum age of an event to forward."
		required:    false
		type: uint: {
			default: 3600
			unit:    "seconds"
		}
	}
	namespaces: {
		description: "Limits the collection to the specified namespaces. If empty, all namespaces are watched."
		required:    false
		type: array: {
			default: []
			items: type: string: examples: ["kube-system"]
		}
	}
	watch_timeout_seconds: {
		description: "Timeout applied to the Kubernetes watch call."
		required:    false
		type: uint: {
			default: 290
			unit:    "seconds"
		}
	}
}
