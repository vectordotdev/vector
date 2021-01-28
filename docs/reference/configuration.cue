package metadata

configuration: #Schema

configuration: {
	data_dir: {
		common: false
		description: """
			The directory used for persisting Vector state, such
			as on-disk buffers, file checkpoints, and more.
			Please make sure the Vector project has write
			permissions to this directory.
			"""
		required: false
		type: string: {
			default: "/var/lib/vector/"
			examples: ["/var/lib/vector", "/var/local/lib/vector/", "/home/user/vector/"]
			syntax: "literal"
		}
	}

	healthchecks: {
		common: false
		description: """
			Configures health checks for all sinks.
			"""
		required: false
		warnings: []
		type: object: {
			examples: []
			options: {
				enabled: {
					common: true
					description: """
						Disables all health checks if false, otherwise sink specific
						option overrides it.
						"""
					required: false
					warnings: []
					type: bool: {
						default: true
					}
				}

				require_healthy: {
					common: false
					description: """
						Exit on startup if any sinks' health check fails. Overridden by
						`--require-healthy` command line flag.
						"""
					required: false
					warnings: []
					type: bool: {
						default: false
					}
				}
			}
		}
	}
}
