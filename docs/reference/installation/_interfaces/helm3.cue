package metadata

installation: _interfaces: "helm3": {
	title:       "Helm 3"
	description: """
		[Helm](\(urls.helm)) is a package manager for Kubernetes that
		facilitates the deployment and management of applications and services
		on Kubernetes clusters.
		"""

	archs: ["x86_64", "ARM64"]

	paths: {
		bin:         null
		bin_in_path: null
		config:      null
	}

	package_manager_name: installation.package_managers.helm.name
	platform_name:        installation.platforms.kubernetes.name

	roles: [Name=string]: {
		commands: {
			_name:                     string
			_controller_resource_type: string
			add_repo:                  #"helm repo add timberio https://packages.timber.io/helm/latest"#
			configure: #"""
				cat <<-'VALUES' > values.yaml
				// The Vector Kubernetes integration automatically defines a
				// kubernetes_logs source that is made available to you.
				// You do not need to define a log source.
				sinks:
				  // Adjust as necessary. By default we use the console sink
				  // to print all data. This allows you to see Vector working.
				  // https://vector.dev/docs/reference/sinks/
				  stdout:
				    type: console
				    inputs: ["kubernetes_logs"]
				    rawConfig: |
				    target = "stdout"
				    encoding = "json"
				VALUES
				"""#
			install:   #"helm install \#(_name) timberio/\#(_name) --devel --values values.yaml --namespace vector --create-namespace"#
			logs:      #"kubectl logs -n vector \#(_controller_resource_type)/\#(_name)"#
			reload:    null
			restart:   #"kubectl rollout restart \#(_controller_resource_type)/\#(_name)"#
			start:     null
			stop:      null
			top:       null
			uninstall: "helm uninstall \(_name) --namespace vector"
			upgrade:   "helm repo update && helm upgrade vector timberio/\(_name) --namespace vector --reuse-values"
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
						with Kubernetes metadata via the
						[Kubernetes API](\#(urls.kubernetes_api)). Collection
						is handled automatically, and it is intended for you to
						adjust your pipeline as	necessary using Vector's
						[sources](\#(urls.vector_sources)),
						[transforms](\#(urls.vector_transforms)), and
						[sinks](\#(urls.vector_sinks)).
						"""#

			commands: {
				_name:                     "vector-agent"
				_controller_resource_type: "daemonset"
			}
			tutorials: installation: [
				{
					title:   "Add the Vector repo"
					command: commands.add_repo
				},
				{
					title:   "Configure Vector"
					command: commands.configure
				},
				{
					title:   "Install Vector"
					command: commands.install
				},
			]
		}

		// aggregator: {
		//  title:       "Aggregator"
		//  description: #"""
		//      The aggregator role is designed to receive and
		//      process data from multiple upstream agents.
		//      Typically these are other Vector agents, but it
		//      could be anything, including non-Vector agents.
		//      By default, we recommend the [`vector` source](\#(urls.vector_source))
		//      since it supports all data types, but it is
		//      recommended to adjust your pipeline as necessary
		//      using Vector's [sources](\#(urls.vector_sources)),
		//      [transforms](\#(urls.vector_transforms)), and
		//      [sinks](\#(urls.vector_sinks)).
		//      """#

		//  commands: {
		//   _name:          "vector-aggregator"
		//   _controller_resource_type: "statefulset"
		//  }
		//  tutorials: installation: [
		//   {
		//    title:   "Add the Vector repo"
		//    command: commands.add_repo
		//   },
		//   {
		//    title: "Configure Vector"
		//    command: commands.configure
		//   },
		//   {
		//    title:   "Install Vector"
		//    command: commands.install
		//   },
		//  ]
		//  variables: config: sources: in: type: "vector"
		// }
	}
}
