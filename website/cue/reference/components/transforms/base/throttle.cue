package metadata

base: components: transforms: throttle: configuration: {
	exclude: {
		description: "A logical condition used to exclude events from sampling."
		required:    false
		type: condition: {}
	}
	key_field: {
		description: """
			The name of the log field whose value will be hashed to determine if the event should be
			rate limited.

			Each unique key will create a bucket of related events to be rate limited separately. If
			left unspecified, or if the event doesn’t have `key_field`, the event be will not be rate
			limited separately.
			"""
		required: false
		type: string: {
			examples: ["{{ message }}", "{{ hostname }}"]
			syntax: "template"
		}
	}
	mode: {
		description: "The throttling mode used to determine each event's contribution against `threshold`."
		required: false
		type: string: {
			default: "events"
			enum: {
				"events": "Throttle by number of events."
				"log_message_bytes": "Throttle by number of bytes in each event's `message` field (logs only)."
				"log_json_bytes": "Throttle by number of estimated bytes for JSON representation of each event (logs only)."
			}
		}
	}
	threshold: {
		description: """
			The number of events allowed for a given bucket per configured `window_secs`.

			Each unique key will have its own `threshold`.
			"""
		required: true
		type: uint: {}
	}
	window_secs: {
		description: "The time window in which the configured `threshold` is applied, in seconds."
		required:    true
		type: float: unit: "seconds"
	}
}
