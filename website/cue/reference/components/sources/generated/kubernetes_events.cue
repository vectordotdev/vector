package metadata

generated: components: sources: kubernetes_events: configuration: {
	dedupe_retention_seconds: {
		description: "Retention window for deduplication state."
		required:    false
		type: uint: {
			default: 900
			unit:    "seconds"
		}
	}
	field_selector: {
		description: "Field selector applied to the events list and watch request."
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
		description: "Path to a kubeconfig file. If omitted, the in-cluster configuration or local kubeconfig is used."
		required:    false
		type: string: examples: ["/path/to/kubeconfig"]
	}
	label_selector: {
		description: "Label selector applied to the events list/watch request."
		required:    false
		type: string: examples: ["type=Warning"]
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
