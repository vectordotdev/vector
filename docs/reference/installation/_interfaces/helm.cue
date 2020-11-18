package metadata

installation: _interfaces: helm: {
	title:       "Helm"
	description: """
		[Helm](\(urls.helm)) is a package manager for Kubernetes that
		facilitates the deployment and management of services on Kubernetes
		clusters.
		"""

	archs: ["x86_64", "ARM64"]
	paths: {
		bin:         "/usr/bin/vector"
		bin_in_path: true
		config:      "configs/vector.{config_format}"
	}
	package_manager_name: installation.package_managers.helm.name
	platform_name:        installation.platforms.kubernetes.name
	roles: {
		_commands: {
			_name:          string
			_resource_type: string
			configure:      #"""
						cat <<-VECTORCFG > \#(paths.config)
						{config}
						VECTORCFG
						"""#
			install: #"""
				 helm repo add timberio-nightly https://packages.timber.io/helm/nightly && \
					helm install vector timberio/\(_name) --devel --values values.yaml --namespace vector --create-namespace
				"""#
			logs:        #"kubectl logs -n vector \#(_resource_type)/\#(_name)"#
			reconfigure: #"kubectl edit \#(_resource_type) \#(_name)"#
			reload:      #"kubectl rollout restart \#(_resource_type)/\#(_name)"#
			restart:     #"kubectl rollout restart \#(_resource_type)/\#(_name)"#
			start:       null
			stop:        null
			uninstall:   "helm uninstall --namespace vector \(_name)"
			upgrade:     "helm upgrade vector timberio/vector --version {version}"
		}
		agent: {
			title:       "Agent"
			description: #"""
						The agent role is designed to collect all Kubernetes
						log data on each Node. Vector runs as a
						[Daemonset](\#(urls.kubernetes_daemonset)) and tails
						logs for the entire Pod, automatically enriching them
						with Kubernetes metdata via the
						[Kubernetes API](\#(urls.kubernetes_api)). Collection
						is handled automatically, and it is intended for you to
						adjust your pipeline as	necessary using Vector's
						[sources](\#(urls.vector_sources)),
						[transforms](\#(urls.vector_transforms)), and
						[sinks](\#(urls.vector_sinks)).
						"""#

			commands: _commands & {
				_name:          "vector-agent"
				_resource_type: "daemonset"
			}
			tutorials: installation: [
				{
					title: "Configure Vector"
					command: #"""
						cat <<-VALUES > values.yaml
						# Configure vect to send the logs from the built-in `kubernetes_logs`
						# source to the stdout.
						vector-\(_role):
						  sinks:
						    stdout:
						      type: console
						      inputs: ["kubernetes_logs"]
						      rawConfig: |
						        target = "stdout"
						        encoding = "json"
						VALUES
						"""#
				},
				{
					title:   "Install Vector"
					command: commands.install
				},
			]
		}
		aggregator: {
			title:       "Aggregator"
			description: #"""
							The aggregator role is designed to receive and
							process data from multiple upstream agents.
							Typically these are other Vector agents, but it
							could be anything, including non-Vector agents.
							By default, we recommend the [`vector` source](\#(urls.vector_source))
							since it supports all data types, but it is
							recommended to adjust your pipeline as necessary
							using Vector's [sources](\#(urls.vector_sources)),
							[transforms](\#(urls.vector_transforms)), and
							[sinks](\#(urls.vector_sinks)).
							"""#

			commands: _commands & {
				_name:          "vector-aggregator"
				_resource_type: "statefulset"
				variables: config: sources: in: type: components.sources.vector.type
			}
			tutorials: installation: [
				{
					title: "Configure Vector"
					command: #"""
						cat <<-VALUES > values.yaml
						vector-aggregator:
						  sinks:
						    stdout:
						      type: console
						      inputs: ["kubernetes_logs"]
						      rawConfig: |
						        target = "stdout"
						        encoding = "json"
						VALUES
						"""#
				},
				{
					title:   "Install Vector"
					command: commands.install
				},
			]
		}
	}
}
