package metadata

configuration: {
	configuration: #Schema
	how_it_works:  #HowItWorks
}

configuration: {
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

	how_it_works: {
		environment_variables: {
			title: "Environment variables"
			body: """
				Vector will interpolate environment variables within your configuration file
				with the following syntax:

				```toml title="vector.toml"
				[transforms.add_host]
				  type = "add_fields"

				  [transforms.add_host.fields]
				    host = "${HOSTNAME}"
				    environment = "${ENV:-development}" # default value when not present
				```
				"""

			sub_sections: [
				{
					title: "Default values"
					body: """
						Default values can be supplied via the `:-` syntax:

						```toml
						option = "${ENV_VAR:-default}"
						```
						"""
				},
				{
					title: "Escaping"
					body: """
						You can escape environment variable by preceding them with a `$` character. For
						example `$${HOSTNAME}` will be treated _literally_ in the above environment
						variable example.
						"""
				},
			]
		}
		formats: {
			title: "Formats"
			body:  """
				Vector supports [TOML](\(urls.toml)), [YAML](\(urls.yaml)), and [JSON](\(urls.json)) to
				ensure Vector fits into your workflow. A side benefit of supporting JSON is the
				enablement of data templating languages like [Jsonnet](\(urls.jsonnet)) and
				[Cue](\(urls.cue)).
				"""
		}
		location: {
			title: "Location"
			body: """
				The location of your Vector configuration file depends on your installation method. For most Linux
				based systems, the file can be found at `/etc/vector/vector.toml`.
				"""
		}
		multiple: {
			title: "Multiple files"
			body:  """
				You can pass multiple configuration files when starting Vector:

				```bash
				vector --config vector1.toml --config vector2.toml
				```

				Or use a [globbing syntax](\(urls.globbing)):

				```bash
				vector --config /etc/vector/*.toml
				```
				"""
		}
	}
}
