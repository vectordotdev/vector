package metadata

_schemaDefinitions: "core::option::Option<vector::sinks::util::service::health::HealthConfig>": object: options: {
	retry_initial_backoff_secs: {
		description: "Initial delay between attempts to reactivate endpoints once they become unhealthy."
		required:    false
		type: uint: {
			default: 1
			unit:    "seconds"
		}
	}
	retry_max_duration_secs: {
		description: "Maximum delay between attempts to reactivate endpoints once they become unhealthy."
		required:    false
		type: uint: {
			default: 3600
			unit:    "seconds"
		}
	}
}
