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
		bin:         null
		bin_in_path: null
		config:      null
	}
	platform_name: installation.platforms.kubernetes.name
	roles: {
		_commands: {
			_resource_type: string
			_role:          string
			configure:      #"""
						cat <<-VECTORCFG > vector-\#(_role).toml
						{config}
						VECTORCFG
						"""#
			install:        "kubectl apply -k ."
			logs:           #"kubectl logs -n vector \#(_resource_type)/vector-\#(_role)"#
			reconfigure:    #"kubectl edit \#(_resource_type) vector-\#(_role)"#
			reload:         #"kubectl rollout restart \#(_resource_type)/vector-\#(_role)"#
			restart:        null
			start:          null
			stop:           null
			top:            null
			uninstall:      "kubectl delete -k ."
			upgrade:        null
		}
		_tutorials: {
			_commands: _
			installation: [
				{
					title: "Create Vector namespace.yaml"
					command: #"""
						cat <<-NAMESPACE > namespace.yaml
						apiVersion: v1
						kind: Namespace
						metadata:
						  name: vector
						NAMESPACE
						"""#
				},
				{
					title: "Create Vector kustomization.yaml"
					command: #"""
						cat <<-KUSTOMIZATION > kustomization.yaml
						namespace: vector
						bases:
						  - github.com/timberio/vector/distribution/kubernetes
						resources:
						  - namespace.yaml
						configMapGenerator:
						  - name: vector-agent-config
						    files:
						      - vector-agent.toml
						KUSTOMIZATION
						"""#
				},
				{
					title:   "Configure Vector"
					command: _commands.configure
				},
				{
					title:   "Install Vector"
					command: _commands.install
				},
			]
		}
		agent: {
			commands: _commands & {
				_resource_type: "daemonset"
				_role:          "agent"
				variables: config: sinks: out: inputs: ["internal_metrics", "kubernetes_logs"]
			}
			tutorials:   _tutorials & {_commands: commands}
			description: "test"
			title:       "Agent"
		}
		aggregator: {
			commands: _commands & {
				_resource_type: "statefulset"
				_role:          "aggregator"
				variables: config: sources: in_upstream: type: components.sources.vector.type
			}
			tutorials:   _tutorials & {_commands: commands}
			description: "test"
			title:       "Aggregator"
		}
	}
}
