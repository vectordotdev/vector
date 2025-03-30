package metadata

configuration: {
	configuration: #Schema
	how_it_works:  #HowItWorks
}

configuration: {
	configuration: base.configuration.configuration

	configuration: {
		// expire_metrics's type is a little bit tricky, we could not generate `uint` from `docs::type_override` metadata macro easily.
		// So we have to define it manually, which is okay because it is already deprecated and it will be deleted soon.
		expire_metrics: {
			common: false
			description: """
				If set, Vector will configure the internal metrics system to automatically
				remove all metrics that have not been updated in the given time.

				If set to a negative value expiration is disabled.
				"""
			required: false
			warnings: ["Deprecated, please use `expire_metrics_secs` instead."]
			type: object: options: {
				secs: {
					common:      true
					required:    false
					description: "The whole number of seconds after which to expire metrics."
					type: uint: {
						default: null
						examples: [60]
						unit: "seconds"
					}
				}
				nsecs: {
					common:      true
					required:    false
					description: "The fractional number of seconds after which to expire metrics."
					type: uint: {
						default: null
						examples: [0]
						unit: "nanoseconds"
					}
				}

			}}}

	how_it_works: {
		environment_variables: {
			title: "Environment variables"
			body: """
				Vector interpolates environment variables within your configuration file
				with the following syntax:

				```toml title="vector.yaml"
				transforms:
					add_host:
						inputs: ["apache_logs"]
						type: "remap"
						source: |
							.host = get_env_var!("HOSTNAME")
				```
				"""

			sub_sections: [
				{
					title: "Default values"
					body: """
						Default values can be supplied using `:-` or `-` syntax:

						```toml
						option = "${ENV_VAR:-default}" # default value if variable is unset or empty
						option = "${ENV_VAR-default}" # default value only if variable is unset
						```
						"""
				},
				{
					title: "Required variables"
					body: """
						Environment variables that are required can be specified using `:?` or `?` syntax:

						```toml
						option = "${ENV_VAR:?err}" # Vector exits with 'err' message if variable is unset or empty
						option = "${ENV_VAR?err}" # Vector exits with 'err' message only if variable is unset
						```
						"""
				},
				{
					title: "Escaping"
					body: """
						You can escape environment variable by preceding them with a `$` character. For
						example `$${HOSTNAME}` or `$$HOSTNAME` is treated literally in the above environment
						variable example.
						"""
				},
			]
		}
		secrets_management: {
			title: "Secrets management"
			body: """
				Vector can retrieve secrets like a password or token by querying an external system in order to
				avoid storing sensitive information in Vector configuration files. Secret backends used to retrieve
				sensitive token are configured in a dedicated section (`secret`). In the rest of the configuration you should use
				the `SECRET[<backend_name>.<secret_key>]` notation to interpolate the secret. Interpolation will happen immediately after
				environment variables interpolation. While Vector supports multiple commands to retrieve secrets, a
				secret backend cannot use the secret interpolation feature for its own configuration. Currently the only supported
				kind of secret backend is the `exec` one that runs an external command to retrieve secrets.

				The following example shows a simple configuration with two backends defined:

				```toml title="vector.yaml"
				secret:
					backend_1:
						type: "exec"
						command: ["/path/to/cmd1", "--some-option"]
					backend_2:
						type: "exec"
						command: ["/path/to/cmd2"]

				sinks:
					dd_logs:
						type: "datadog_logs"
						default_api_key: "SECRET[backend_1.dd_api_key]"

					splunk:
						type: "splunk_hec"
						default_token: "SECRET[backend_2.splunk_token]"
				```

				In that example Vector will retrieve the `dd_api_key` from `backen_1` and `splunk_token` from `backend_2`.
				"""

			sub_sections: [
				{
					title: "The `exec` backend"
					body:  """
						When using the `exec` type for a secret backend Vector and the external command are communicating using
						the standard input and output. The communication is using plain text JSON. Vector spawns the specified
						command, writes the secret retrieval query using JSON on the standard input of the child process and
						reads its standard output expecting a JSON object. Standard error from the child process is read and
						logged by Vector and can be used for troubleshooting.

						The JSON object used for communication between an `exec` backend and Vector shall comply with the
						[Datadog Agent executable API](\(urls.datadog_agent_exec_api)).

						For example a query would look like:

						```json
						{
							"version": "1.0",
							"secrets": ["dd_api_key", "another_dd_api_key"]
						}
						```

						A possible reply from a backend could then be:

						```json
						{
							"dd_api_key": {"value": "A_DATADOG_API_KEY", "error": null},
							"another_dd_api_key": {"value": null, "error": "unable to retrieve secret"}
						}
						```

						If a backend writes a JSON that does not follow the expected structure or reports an error for a
						given secret Vector will refuse to load the configuration.

						Currently Vector will always query backend with `"version": "1.0"`.
						"""
				},
			]
		}
		formats: {
			title: "Formats"
			body:  """
				Vector supports [YAML](\(urls.yaml)), [TOML](\(urls.toml)), and [JSON](\(urls.json)) to
				ensure Vector fits into your workflow. A side benefit of supporting YAML and JSON is that they
				enable you to use data templating languages such as [ytt](\(urls.ytt)), [Jsonnet](\(urls.jsonnet)) and
				[Cue](\(urls.cue)).
				"""
		}
		location: {
			title: "Location"
			body: """
				The location of your Vector configuration file depends on your installation method. For most Linux
				based systems, the file can be found at `/etc/vector/vector.yaml`.

				All files in `/etc/vector` are user configuration files and can be safely overridden to craft your
				desired Vector configuration.
				"""
		}
		multiple: {
			title: "Multiple files"
			body:  """
				You can pass multiple configuration files when starting Vector:

				```bash
				vector --config vector1.yaml --config vector2.yaml
				```

				Or use a [globbing syntax](\(urls.globbing)):

				```bash
				vector --config /etc/vector/*.yaml
				```
				"""
		}
		automatic_namespacing: {
			title: "Automatic namespacing of component files"
			body: """
				You can split your configuration files in component-type related folders.

				For example, you can create the sink `foo` in the folder `/path/to/vector/config/sinks/foo.toml` and
				configure it as follows:

				```toml
				type: "sink_type"
				# here the sinks options
				```

				You can do the same for other kinds of components like `sources`, `transforms`, `tests` and `enrichment_tables`.

				For vector to find and load the different configuration files, you need to load your configuration with
				the `--config-dir` argument.

				```bash
				vector --config-dir /path/to/vector/config
				```
				"""
		}
		wildcards: {
			title: "Wildcards in component names"
			body: """
				Vector supports wildcard characters (`*`) in component names when building your topology.

				For example:

				```yaml
				sources:
					app1_logs:
						type: "file"
						includes: ["/var/log/app1.log"]

					app2_logs:
						type: "file"
						includes: ["/var/log/app.log"]

					system_logs:
						type: "file"
						includes: ["/var/log/system.log"]

				sinks:
					app_logs:
						type: "datadog_logs"
						inputs: ["app*"]

					archive:
						type: "aws_s3"
						inputs: ["*_logs"]
				```
				"""
		}
	}
}
