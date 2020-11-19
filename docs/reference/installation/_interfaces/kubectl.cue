package metadata

installation: _interfaces: kubectl: {
	title:       "kubectl"
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

	roles: [Name=string]: {
		commands: {
			_resource_type: string
			_role:          string
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
			verify_config:  "kubectl kustomize"
		}

		tutorials: {
			installation: [
				{
					title:   "Define Vector's namespace"
					command: "kubectl create namespace --dry-run=client -oyaml vector > namespace.yaml"
				},
				{
					title: "Prepare kustomization"
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
					command: commands.configure
				},
				{
					title:   "Verify the config"
					command: commands.verify_config
				},
				{
					title:   "Install Vector"
					command: commands.install
				},
			]
		}
	}

	roles: {
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

			commands: {
				_resource_type: "daemonset"
				_role:          "agent"
				configure: #"""
					cat <<-'VECTORCFG' > vector-agent.toml
					# The Vector Kubernetes integration automatically defines a
					# kubernetes_logs source that is made available to you.
					# You do not need to define a log source.

					{config}
					VECTORCFG
					"""#
			}
			variables: config: sinks: out: inputs: ["kubernetes_logs"]
		}

		// aggregator: {
		//  title:       "Aggregator"
		//  description: #"""
		//   The aggregator role is designed to receive and
		//   process data from multiple upstream agents.
		//   Typically these are other Vector agents, but it
		//   could be anything, including non-Vector agents.
		//   By default, we recommend the [`vector` source](\#(urls.vector_source))
		//   since it supports all data types, but it is
		//   recommended to adjust your pipeline as necessary
		//   using Vector's [sources](\#(urls.vector_sources)),
		//   [transforms](\#(urls.vector_transforms)), and
		//   [sinks](\#(urls.vector_sinks)).
		//   """#

		//  commands: {
		//   _resource_type: "statefulset"
		//   _role:          "aggregator"
		//   configure: #"""
		//    cat <<-'VECTORCFG' > vector-aggregator.toml
		//    {config}
		//    VECTORCFG
		//    """#
		//  }
		//  variables: config: sources: in_upstream: type: components.sources.vector.type
		// }
	}
}
