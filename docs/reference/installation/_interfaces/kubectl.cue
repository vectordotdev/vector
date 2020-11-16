package metadata

installation: _interfaces: kubectl: {
	title:       "Kubectl"
	description: """
		The [Kubernetes command-line tool](\(urls.kubectl)), kubectl, allows
		users to run commands against Kubernetes clusters facilitating in
		application deployment, scaling, monitoring, and introspection.
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
			commands: _commands & {
				_role: "agent"
				variables: config: sinks: out: inputs: ["internal_metrics", "kubernetes_logs"]
			}
			description: "test"
			title:       "Agent"
		}
		aggregator: {
			commands: _commands & {
				_role: "aggregator"
				variables: config: sources: in_upstream: type: components.sources.vector.type
			}
			description: "test"
			title:       "Aggregator"
		}
	}
}
