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
		}
	}
}
