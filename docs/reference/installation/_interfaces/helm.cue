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
	platform_name: installation.platforms.kubernetes.name
	roles: {
		_commands: {
			_role:     string
			configure: #"""
						cat <<-VECTORCFG > \#(paths.config)
						{config}
						VECTORCFG
						"""#
			install:   "kubectl apply -k ."
			logs:      #"kubectl logs -n vector daemonset/vector-\#(_role)"#
			reload:    #"kubectl rollout restart daemonset/vector-\#(_role)"#
			start:     null
			stop:      null
			uninstall: "kubectl delete -k ."
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
				_role: "agent"
				variables: config: sources: in: type: components.sources.kubernetes_logs.type
			}
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
				_role: "aggregator"
				variables: config: sources: in: type: components.sources.vector.type
			}
		}
		god_mode: {
			commands: _commands & {
				_role: "god_mode"
				variables: config: sources: in: type: components.sources.vector.type
			}
			description: "test"
			title:       "God Mode"
		}
	}
}
