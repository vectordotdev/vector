package metadata

administration: interfaces: kubectl: {
	title:       "kubectl"
	description: """
		The [Kubernetes command-line tool](\(urls.kubectl)), kubectl, allows
		users to run commands against Kubernetes clusters facilitating
		application deployment, scaling, monitoring, and introspection.
		"""

	archs: ["x86_64", "ARM64"]
	paths: {
		bin:         null
		bin_in_path: null
		config:      "vector.toml"
	}
	platform_name: "kubernetes"

	role_implementations: [Name=string]: {
		commands: {
			_deployment_variant:       string
			_vector_version:           "0.41"
			_namespace:                string | *"vector"
			_controller_resource_type: string
			_controller_resource_name: string | *_deployment_variant
			_kustomization_ref:        "v\(_vector_version)"
			_kustomization_base:       string | *"github.com/vectordotdev/vector/distribution/kubernetes/\(_deployment_variant)?ref=\(_kustomization_ref)"
			_configmap_name:           string | *"\(_controller_resource_name)-config"
			_configmap_file_name:      string | *"\(_controller_resource_name).toml"
			_config_header:            string | *""
			_vector_image_version:     "\(_vector_version).X"
			_vector_image_flavor:      "debian"
			_vector_image_tag:         "\(_vector_image_version)-\(_vector_image_flavor)"
			install:                   "kubectl apply -k ."
			logs:                      "kubectl logs -n \(_namespace) \(_controller_resource_type)/\(_controller_resource_name)"
			reload:                    null
			restart:                   "kubectl rollout restart -n \(_namespace) \(_controller_resource_type)/\(_controller_resource_name)"
			start:                     null
			stop:                      null
			top:                       null
			uninstall:                 "kubectl delete -k ."
			upgrade:                   null
			verify_config:             "kubectl kustomize"
			prepare_namespace:         "kubectl create namespace --dry-run=client -oyaml \(_namespace) > namespace.yaml"
			prepare_kustomization:     #"""
				cat <<-'KUSTOMIZATION' > kustomization.yaml
				# Override the namespace of all of the resources we manage.
				namespace: \#(_namespace)

				bases:
				  # Include Vector recommended base (from git).
				  - \#(_kustomization_base)

				images:
				  # Override the Vector image to avoid use of the sliding tag.
				  - name: timberio/vector
				    newName: timberio/vector
				    newTag: \#(_vector_image_tag)

				resources:
				  # A namespace to keep the resources at.
				  - namespace.yaml

				configMapGenerator:
				  # Provide a custom `ConfigMap` for Vector.
				  - name: \#(_configmap_name)
				    files:
				      - \#(_configmap_file_name)

				generatorOptions:
				  # We do not want a suffix at the `ConfigMap` name.
				  disableNameSuffixHash: true
				KUSTOMIZATION
				"""#
			configure:                 #"""
				cat <<-'VECTORCFG' > \#(_configmap_file_name)
				\#(_config_header){config}
				VECTORCFG
				"""#
		}

		tutorials: {
			installation: [
				{
					title:   "Define Vector's namespace"
					command: commands.prepare_namespace
				},
				{
					title:   "Prepare kustomization"
					command: commands.prepare_kustomization
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

	role_implementations: {
		agent: {
			title:       "Agent"
			description: #"""
						The agent role is designed to collect all Kubernetes
						log data on each Node. Vector runs as a
						[DaemonSet](\#(urls.kubernetes_daemonset)) and tails
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
				_deployment_variant:       "vector-agent"
				_controller_resource_type: "daemonset"
				_config_header: """
					# The Vector Kubernetes integration automatically defines a
					# `kubernetes_logs` source that is made available to you.
					# You do not need to define a log source.
					\n
					"""
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
		//   _deployment_variant:       "vector-aggregator"
		//   _controller_resource_type: "statefulset"
		//  }
		//  variables: config: sources: in_upstream: type: "vector"
		// }
	}
}
